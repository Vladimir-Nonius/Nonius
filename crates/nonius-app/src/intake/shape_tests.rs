//! Тесты сравнения формы: числа — как у настоящих деталей.

use super::*;

fn shape(volume: f64, dims: [f64; 3]) -> Shape {
    Shape {
        volume,
        area: None,
        dims,
        faces: None,
    }
}

/// Коллектор из проверочной папки: вторая версия — те же габариты, чуть
/// меньше металла, меньше граней.
fn manifold() -> (Shape, Shape) {
    let v1 = Shape {
        faces: Some(352),
        ..shape(439_112.9, [61.0, 148.0, 420.0])
    };
    let v2 = Shape {
        volume: 435_724.8,
        faces: Some(344),
        ..v1
    };
    (v1, v2)
}

fn labels(i: usize) -> String {
    ["вал", "вал_v2", "вал_v3"][i].to_string()
}

#[test]
fn passport_becomes_a_shape() {
    use nonius_core::meta::{BoundingBox, GeometrySummary};
    let mut g = GeometrySummary {
        volume_mm3: Some(1000.0),
        ..Default::default()
    };
    assert_eq!(Shape::of(&g), None, "без габаритов сравнивать нечем");
    g.bbox = Some(BoundingBox {
        min: [0.0, -5.0, 0.0],
        max: [30.0, 5.0, 20.0],
    });
    // Как деталь повёрнута — не важно: габариты по возрастанию.
    assert_eq!(Shape::of(&g).unwrap().dims, [10.0, 20.0, 30.0]);
    g.volume_mm3 = Some(0.0);
    assert_eq!(Shape::of(&g), None, "поверхность без объёма — не деталь");
}

#[test]
fn revision_of_a_part_is_similar() {
    let (v1, v2) = manifold();
    assert_eq!(
        compare(&v1, &v2),
        Likeness::Similar("объём −0,8 %, габариты те же".into())
    );
    // Вал удлинили на десятую: два габарита те же, третий вырос.
    let long = shape(11_000.0, [20.0, 20.0, 110.0]);
    let Likeness::Similar(how) = compare(&shape(10_000.0, [20.0, 20.0, 100.0]), &long) else {
        panic!("удлинённый вал — тот же вал");
    };
    assert_eq!(how, "объём +9,1 %, габариты 20×20×100 мм → 20×20×110 мм");
}

#[test]
fn reworked_part_is_similar() {
    // Настоящая пара: впускной коллектор и он же «доработаный».
    let old = shape(426_832.2, [68.0, 89.6, 397.8]);
    let new = shape(439_112.9, [70.0, 84.0, 397.9]);
    assert_eq!(
        compare(&old, &new),
        Likeness::Similar("объём +2,8 %, габариты 68×90×398 мм → 70×84×398 мм".into())
    );
}

#[test]
fn copy_is_not_a_revision() {
    let (v1, _) = manifold();
    assert_eq!(compare(&v1, &v1), Likeness::Same);
    // Те же габариты и объём, но граней стало больше — уже не копия.
    let other = Shape {
        faces: Some(353),
        ..v1
    };
    assert!(matches!(compare(&v1, &other), Likeness::Similar(_)));
}

#[test]
fn other_part_is_different() {
    let shaft = shape(10_000.0, [20.0, 20.0, 100.0]);
    let plate = shape(10_000.0, [5.0, 40.0, 50.0]);
    assert_eq!(
        compare(&shaft, &plate),
        Likeness::Different("габариты 20×20×100 мм и 5,0×40×50 мм".into())
    );
    // Габариты те же, а металла вдвое меньше: труба вместо прутка.
    let pipe = shape(5_000.0, [20.0, 20.0, 100.0]);
    assert_eq!(
        compare(&shaft, &pipe),
        Likeness::Different("объём −50,0 %".into())
    );
}

#[test]
fn group_without_shapes_stays_unchecked() {
    let (v1, _) = manifold();
    let known = |i: usize| (i == 0).then_some(v1);
    let parts = split(&[0, 1], &known, &labels, &|_, _| None);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].members, [0, 1]);
    assert_eq!(parts[0].verdict, None);
    assert!(parts[0].notes.is_empty());
}

#[test]
fn similar_versions_are_confirmed() {
    let (v1, v2) = manifold();
    let shapes = [v1, v2];
    let parts = split(&[0, 1], &|i| Some(shapes[i]), &labels, &|_, _| None);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].verdict, Some(ShapeVerdict::Confirmed));
    assert_eq!(
        parts[0].notes,
        ["подтверждено формой, «вал» → «вал_v2»: объём −0,8 %, габариты те же"]
    );
}

#[test]
fn stranger_leaves_the_group() {
    let (v1, v2) = manifold();
    let shapes = [v1, shape(300.0, [5.0, 10.0, 10.0]), v2];
    let parts = split(&[0, 1, 2], &|i| Some(shapes[i]), &labels, &|_, _| None);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].members, [0, 2]);
    assert_eq!(parts[0].verdict, Some(ShapeVerdict::Confirmed));
    assert_eq!(parts[1].members, [1]);
    assert_eq!(parts[1].verdict, Some(ShapeVerdict::Different));
    assert_eq!(parts[1].rival, Some(0), "о паре стоит спросить");
    assert!(parts[1].notes[0].starts_with("имя похоже на «вал», но форма другая: габариты"));
}

#[test]
fn copies_are_reported() {
    let (v1, v2) = manifold();
    let shapes = [v1, v1, v2];
    let parts = split(&[0, 1, 2], &|i| Some(shapes[i]), &labels, &|_, _| None);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].verdict, Some(ShapeVerdict::Copy));
    assert_eq!(
        parts[0].notes[0],
        "форма у «вал_v2» такая же, как у «вал»: возможно, это копия"
    );
    assert!(parts[0].notes[1].starts_with("подтверждено формой, «вал_v2» → «вал_v3»"));
}

#[test]
fn human_word_beats_shape_and_names() {
    let (v1, v2) = manifold();
    let plug = shape(300.0, [5.0, 10.0, 10.0]);
    // Форма похожа, а человек сказал «нет»: две детали, и вопросов больше нет.
    let shapes = [v1, v2];
    let parts = split(&[0, 1], &|i| Some(shapes[i]), &labels, &|_, _| Some(false));
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[1].notes, ["вы ответили: это не версия «вал»"]);
    assert_eq!((parts[1].verdict, parts[1].rival), (None, None));
    // Форма разная, а человек сказал «да»: одна деталь, но не «подтверждено».
    let shapes = [v1, plug];
    let parts = split(&[0, 1], &|i| Some(shapes[i]), &labels, &|_, _| Some(true));
    assert_eq!(parts.len(), 1);
    assert!(parts[0].settled);
    assert_eq!(parts[0].verdict, None);
    assert_eq!(
        parts[0].notes.last().unwrap(),
        "вы ответили: это версии одной детали"
    );
    // Формы нет вовсе — ответ всё равно действует.
    let parts = split(&[0, 1], &|_| None, &labels, &|_, _| Some(false));
    assert_eq!(parts.len(), 2);
}
