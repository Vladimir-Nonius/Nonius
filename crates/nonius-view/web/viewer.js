// Окно просмотра Nonius: деталь крутится мышью, изменения подсвечены цветом.
// Модель приходит файлом glTF (/model.glb); роль слоя — в extras узла.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';

// Цвета — как на картинке изменений (nonius_occt::render::colors).
const ROLES = {
  unchanged: { name: 'Без изменений', color: '#b0b6be', edge: '#56606c' },
  added: { name: 'Добавлено', color: '#22aa4c', edge: '#0f5c27' },
  removed: { name: 'Убрано', color: '#d63031', edge: '#7a1414' },
};
const ORDER = ['unchanged', 'added', 'removed'];
const XRAY_OPACITY = 0.35;

// Направления взгляда. Модель в осях glTF: Y вверх, «спереди» — с +Z.
const VIEWS = {
  iso: [1, 0.85, 1],
  front: [0, 0, 1],
  top: [0, 1, 1e-4],
  right: [1, 0, 0],
};

const canvas = document.getElementById('view');
const message = document.getElementById('message');

function showMessage(text, isError) {
  message.textContent = text;
  message.classList.toggle('error', Boolean(isError));
  message.hidden = !text;
}

let renderer;
try {
  renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
} catch (e) {
  showMessage('Браузер не умеет показывать 3D (WebGL выключен). Откройте ссылку в Chrome, Edge или Firefox.', true);
  throw e;
}
renderer.setPixelRatio(window.devicePixelRatio);

const scene = new THREE.Scene();
// Параллельная проекция, как в CAD: виды «сверху», «спереди» без перспективы.
const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.001, 100);
let aspect = 1;
let radius = 1; // радиус шара, в который вписана деталь
scene.add(camera);
scene.add(new THREE.HemisphereLight(0xffffff, 0x8a929c, 1.6));
// Свет «от головы»: куда ни поверни деталь, видимая сторона освещена.
const headlight = new THREE.DirectionalLight(0xffffff, 1.8);
headlight.position.set(-0.4, 0.6, 1);
camera.add(headlight);

const controls = new OrbitControls(camera, canvas);
controls.mouseButtons = {
  LEFT: THREE.MOUSE.ROTATE,
  MIDDLE: THREE.MOUSE.PAN,
  RIGHT: THREE.MOUSE.PAN,
};
controls.zoomToCursor = true;
controls.addEventListener('change', draw);

const layers = {}; // роль → { group, solid, edges, info }
const bounds = new THREE.Box3();
let changed = false;

function draw() {
  renderer.render(scene, camera);
}

/** Кадр камеры: деталь целиком влезает и по ширине, и по высоте. */
function frame() {
  const half = (radius * 1.08) / Math.min(1, aspect);
  camera.left = -half * aspect;
  camera.right = half * aspect;
  camera.top = half;
  camera.bottom = -half;
  camera.updateProjectionMatrix();
}

function resize() {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  renderer.setSize(w, h, false);
  aspect = w / Math.max(h, 1);
  frame();
  draw();
}
window.addEventListener('resize', resize);

/** Показать деталь целиком, глядя по направлению `dir`. */
function look(dir) {
  if (bounds.isEmpty()) return;
  const center = bounds.getCenter(new THREE.Vector3());
  radius = Math.max(bounds.getSize(new THREE.Vector3()).length() / 2, 1e-6);
  const distance = radius * 4;
  const d = new THREE.Vector3(...dir).normalize();
  camera.position.copy(center).addScaledVector(d, distance);
  camera.near = radius * 0.01;
  camera.far = distance + radius * 4;
  camera.zoom = 1;
  frame();
  controls.target.copy(center);
  controls.update();
  draw();
}

function roleOf(object) {
  for (let o = object; o; o = o.parent) {
    if (o.userData && o.userData.role) return o.userData;
  }
  return { role: 'unchanged' };
}

function material(role) {
  const m = new THREE.MeshStandardMaterial({
    color: ROLES[role].color,
    metalness: 0,
    roughness: 0.65,
    side: THREE.DoubleSide,
  });
  if (role === 'unchanged') {
    // Серое отодвигаем назад: на общих гранях цветное выигрывает.
    m.polygonOffset = true;
    m.polygonOffsetFactor = 1;
    m.polygonOffsetUnits = 1;
  }
  return m;
}

function setXray(on) {
  const layer = layers.unchanged;
  if (!layer) return;
  const transparent = on && changed;
  layer.solid.forEach((mesh) => {
    mesh.material.transparent = transparent;
    mesh.material.opacity = transparent ? XRAY_OPACITY : 1;
    mesh.material.depthWrite = !transparent;
    mesh.material.needsUpdate = true;
    mesh.renderOrder = transparent ? 1 : 0;
  });
  layer.edges.forEach((line) => {
    line.material.opacity = transparent ? 0.35 : 0.6;
  });
  draw();
}

function formatVolume(v) {
  if (!(v > 0)) return '';
  const text = v >= 100 ? Math.round(v).toLocaleString('ru-RU') : v.toFixed(2).replace('.', ',');
  return `${text} мм³`;
}

function buildPanel() {
  const box = document.getElementById('layers');
  box.textContent = '';
  for (const role of ORDER) {
    const layer = layers[role];
    if (!layer) continue;
    const row = document.createElement('label');
    row.className = 'layer';
    const check = document.createElement('input');
    check.type = 'checkbox';
    check.checked = true;
    check.addEventListener('change', () => {
      layer.group.visible = check.checked;
      draw();
    });
    const swatch = document.createElement('span');
    swatch.className = 'swatch';
    swatch.style.background = ROLES[role].color;
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = role === 'unchanged' && !changed ? 'Деталь' : ROLES[role].name;
    const value = document.createElement('span');
    value.className = 'value';
    const pieces = layer.info.pieces > 1 && role !== 'unchanged' ? `, мест: ${layer.info.pieces}` : '';
    // Серое при изменениях — вся новая версия, её объём здесь сбивает с толку.
    const volume = role === 'unchanged' && changed ? '' : formatVolume(layer.info.volume_mm3);
    value.textContent = volume + pieces;
    row.append(check, swatch, name, value);
    box.append(row);
  }
  document.getElementById('xray-row').hidden = !changed;
  document.getElementById('no-changes').hidden = changed;
  document.getElementById('xray').addEventListener('change', (e) => setXray(e.target.checked));
  for (const button of document.querySelectorAll('[data-view]')) {
    button.addEventListener('click', () => look(VIEWS[button.dataset.view]));
  }
  document.getElementById('panel').hidden = false;
}

function addModel(root) {
  const meshes = [];
  root.traverse((o) => {
    if (o.isMesh) meshes.push(o);
  });
  for (const mesh of meshes) {
    const info = roleOf(mesh);
    const role = ROLES[info.role] ? info.role : 'unchanged';
    if (!layers[role]) {
      const group = new THREE.Group();
      scene.add(group);
      layers[role] = { group, solid: [], edges: [], info };
    }
    const layer = layers[role];
    mesh.updateWorldMatrix(true, false);
    const geometry = mesh.geometry.clone().applyMatrix4(mesh.matrixWorld);
    // Вершины у каждой грани свои: нормали гладкие внутри грани, рёбра острые.
    if (!geometry.getAttribute('normal')) geometry.computeVertexNormals();
    const solid = new THREE.Mesh(geometry, material(role));
    const edges = new THREE.LineSegments(
      new THREE.EdgesGeometry(geometry, 25),
      new THREE.LineBasicMaterial({ color: ROLES[role].edge, transparent: true, opacity: 0.6 }),
    );
    layer.group.add(solid, edges);
    layer.solid.push(solid);
    layer.edges.push(edges);
    geometry.computeBoundingBox();
    bounds.union(geometry.boundingBox);
  }
  changed = Boolean(layers.added || layers.removed);
}

async function loadInfo() {
  try {
    const info = await (await fetch('/info.json')).json();
    document.querySelector('#title h1').textContent = info.title || 'Nonius';
    document.querySelector('#title p').textContent = info.subtitle || '';
    if (info.title) document.title = `${info.title} — Nonius`;
  } catch {
    document.querySelector('#title p').textContent = '';
  }
}

async function main() {
  resize();
  loadInfo();
  let gltf;
  try {
    gltf = await new GLTFLoader().loadAsync('/model.glb');
  } catch (e) {
    showMessage('Не удалось загрузить модель. Если окно nonius закрыто — запустите «nonius view» снова.', true);
    throw e;
  }
  addModel(gltf.scene);
  if (bounds.isEmpty()) {
    showMessage('В модели нет геометрии.', true);
    return;
  }
  showMessage('');
  buildPanel();
  setXray(true);
  look(VIEWS.iso);
  canvas.addEventListener('dblclick', () => look(camera.position.clone().sub(controls.target).toArray()));
}

main();
