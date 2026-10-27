//! Окно просмотра Nonius: деталь крутится мышью, изменения подсвечены цветом.
//!
//! Программа поднимает маленький веб-сервер только для этого компьютера
//! (`127.0.0.1`, порт выбирается свободный) и открывает страницу в браузере.
//! Страница и библиотека 3D-графики [three.js](https://threejs.org) (MIT)
//! встроены в программу — интернет не нужен.
//!
//! Модель приходит готовым файлом glTF (`.glb`) — его пишет модуль-отрисовщик
//! (например, `scene-occt`). Сам просмотрщик о геометрии и OpenCascade не знает:
//! он показывает слои модели, а роль слоя (`unchanged`, `added`, `removed`)
//! берёт из `extras` узла.

use std::borrow::Cow;
use std::io;
use std::net::SocketAddr;
use std::process::Command;

use tiny_http::{Header, Request, Response, Server};

/// Что показать в окне.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    /// Заголовок: имя детали.
    pub title: String,
    /// Подзаголовок: «v1 → v2».
    pub subtitle: String,
    /// Модель glTF (`.glb`).
    pub model: Vec<u8>,
}

/// Встроенный файл страницы.
struct Asset {
    path: &'static str,
    mime: &'static str,
    body: &'static [u8],
}

const JS: &str = "text/javascript; charset=utf-8";

const ASSETS: &[Asset] = &[
    Asset {
        path: "/",
        mime: "text/html; charset=utf-8",
        body: include_bytes!("../web/index.html"),
    },
    Asset {
        path: "/viewer.js",
        mime: JS,
        body: include_bytes!("../web/viewer.js"),
    },
    Asset {
        path: "/three/three.module.min.js",
        mime: JS,
        body: include_bytes!("../web/three/three.module.min.js"),
    },
    Asset {
        path: "/three/addons/controls/OrbitControls.js",
        mime: JS,
        body: include_bytes!("../web/three/addons/controls/OrbitControls.js"),
    },
    Asset {
        path: "/three/addons/loaders/GLTFLoader.js",
        mime: JS,
        body: include_bytes!("../web/three/addons/loaders/GLTFLoader.js"),
    },
    Asset {
        path: "/three/addons/utils/BufferGeometryUtils.js",
        mime: JS,
        body: include_bytes!("../web/three/addons/utils/BufferGeometryUtils.js"),
    },
];

/// Ответ на запрос страницы: тип содержимого и тело. `None` — нет такого адреса.
pub fn route<'a>(path: &str, page: &'a Page) -> Option<(&'static str, Cow<'a, [u8]>)> {
    // Параметры после «?» не нужны: адреса фиксированные.
    let path = path.split(['?', '#']).next().unwrap_or("/");
    match path {
        "/model.glb" => Some(("model/gltf-binary", Cow::Borrowed(&page.model))),
        "/info.json" => {
            let info = serde_json::json!({ "title": page.title, "subtitle": page.subtitle });
            Some((
                "application/json; charset=utf-8",
                Cow::Owned(info.to_string().into_bytes()),
            ))
        }
        "/index.html" => route("/", page),
        _ => ASSETS
            .iter()
            .find(|a| a.path == path)
            .map(|a| (a.mime, Cow::Borrowed(a.body))),
    }
}

/// Запущенное окно просмотра.
pub struct Viewer {
    server: Server,
    addr: SocketAddr,
    page: Page,
}

impl Viewer {
    /// Поднять сервер на свободном порту этого компьютера.
    pub fn start(page: Page) -> io::Result<Self> {
        let server = Server::http("127.0.0.1:0").map_err(io::Error::other)?;
        let addr = server
            .server_addr()
            .to_ip()
            .ok_or_else(|| io::Error::other("сервер просмотра не получил адрес"))?;
        Ok(Self { server, addr, page })
    }

    /// Адрес страницы: `http://127.0.0.1:<порт>/`.
    pub fn url(&self) -> String {
        format!("http://{}/", self.addr)
    }

    /// Порт сервера.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Отвечать на запросы, пока программу не закроют.
    pub fn run(&self) {
        for request in self.server.incoming_requests() {
            // Оборванное соединение браузера — не повод останавливать просмотр.
            let _ = self.respond(request);
        }
    }

    /// Ответить на один запрос.
    fn respond(&self, request: Request) -> io::Result<()> {
        // Чужие сайты не должны читать модель через подмену имени (DNS rebinding):
        // принимаем только обращения к 127.0.0.1 и localhost.
        if !self.host_allowed(&request) {
            return request.respond(Response::empty(403));
        }
        match route(request.url(), &self.page) {
            Some((mime, body)) => {
                let response = Response::from_data(body.into_owned())
                    .with_header(header("Content-Type", mime))
                    .with_header(header("Cache-Control", "no-store"));
                request.respond(response)
            }
            None => {
                request.respond(Response::from_string("нет такой страницы").with_status_code(404))
            }
        }
    }

    fn host_allowed(&self, request: &Request) -> bool {
        let port = self.addr.port();
        request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Host"))
            .is_some_and(|h| {
                let host = h.value.as_str();
                host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
            })
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("заголовок из ASCII")
}

/// Открыть адрес в браузере по умолчанию.
pub fn open_browser(url: &str) -> io::Result<()> {
    let status = if cfg!(windows) {
        // `start` — встроенная команда cmd; пустая строка — заголовок окна.
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()?
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).status()?
    } else {
        Command::new("xdg-open").arg(url).status()?
    };
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("браузер не открылся"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Page {
        Page {
            title: "кронштейн".into(),
            subtitle: "v1 → v2".into(),
            model: b"glTF-model".to_vec(),
        }
    }

    #[test]
    fn every_asset_is_served() {
        let page = page();
        for asset in ASSETS {
            let (mime, body) = route(asset.path, &page).unwrap();
            assert_eq!(mime, asset.mime);
            assert!(!body.is_empty(), "{}", asset.path);
        }
        assert_eq!(route("/index.html", &page), route("/", &page));
        assert_eq!(route("/?t=1", &page), route("/", &page));
    }

    #[test]
    fn model_and_info() {
        let page = page();
        let (mime, body) = route("/model.glb", &page).unwrap();
        assert_eq!(mime, "model/gltf-binary");
        assert_eq!(&*body, b"glTF-model");

        let (_, body) = route("/info.json", &page).unwrap();
        let info: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(info["title"], "кронштейн");
        assert_eq!(info["subtitle"], "v1 → v2");
    }

    #[test]
    fn unknown_paths_are_not_found() {
        let page = page();
        for path in [
            "/../Cargo.toml",
            "/three/../../src/lib.rs",
            "/favicon.ico",
            "/web/index.html",
        ] {
            assert!(route(path, &page).is_none(), "{path}");
        }
    }

    #[test]
    fn page_imports_only_served_files() {
        // Каждый модуль, который подключают страница и аддоны three.js, есть среди встроенных.
        let html = std::str::from_utf8(route("/", &page()).unwrap().1.as_ref())
            .unwrap()
            .to_owned();
        assert!(html.contains("\"three\": \"/three/three.module.min.js\""));
        assert!(html.contains("\"three/addons/\": \"/three/addons/\""));
        assert!(html.contains("src=\"/viewer.js\""));
        let js = std::str::from_utf8(include_bytes!("../web/viewer.js")).unwrap();
        for line in js.lines().filter(|l| l.starts_with("import ")) {
            let spec = line.split(['\'', '"']).nth(1).unwrap();
            let path = spec.replacen("three/addons/", "/three/addons/", 1);
            let path = if spec == "three" {
                "/three/three.module.min.js".into()
            } else {
                path
            };
            assert!(route(&path, &page()).is_some(), "{spec}");
        }
    }
}
