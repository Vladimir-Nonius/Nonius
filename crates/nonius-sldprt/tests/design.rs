//! Замысел из файла: файлы собираются прямо в тесте, по образцу настоящих
//! (`docs/formats/sldprt-design.md`). Имена и пути в них выдуманные.

use nonius_core::contract;
use nonius_core::extension::{DesignSource, Registry};
use nonius_sldprt::design::{DocKind, Mark, Piece, Tag, Why};
use nonius_sldprt::{Container, Design, Doubt, SldprtDesign};

const KEYWORDS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Keywords id="1706104060" Name="Вал"><Configuration id="0" Name="По умолчанию" Description="По_умолчанию" Type="ConfigurationManager" Material="2024-T3"/><Extrusion id="1910" Name="Бобышка-Вытянуть2" Dissectable="true" DissectableChildren="610" DissectableRoot="true"><Dimension Name="D1">5</Dimension></Extrusion><Extrusion id="53" Name="Бобышка-Вытянуть1" Type="Бобышка-Вытянуть"><Dimension Name="D1">15</Dimension></Extrusion><Feature id="-1" Name="" Type="Sheet-Metal Master"/><Feature id="-1" Name="{0F2B6F1A-3C4D-4E5F-8A9B-0C1D2E3F4A5B}" Type="Атрибут"/><Feature id="1" Name="Примечания" Type="Примечания"/><Feature id="2" Name="Спереди" Type="Плоскость"/><Feature id="2127" Name="Скругление20" Type="Скругление"><Dimension Name="D1">8</Dimension><Dimension Name="D2">14.5</Dimension></Feature><Feature id="2130" Name="Фаска1" Type="Фаска"><Dimension Name="D1">R2</Dimension><Dimension Name="D2">45°</Dimension><Dimension Name="D3">17/32"</Dimension></Feature><Feature id="34" Name="Уравнения" Type="Уравнения"/><Sketch id="5" Name="Исходная точка" Type="Исходная точка"/><Sketch id="610" Name="Эскиз28" Dissectable="true"><Dimension Name="D1">&lt;MOD-DIAM&gt;42</Dimension></Sketch></Keywords>"#;

fn features(needs_update: &str, external: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<swSolidWorks xmlns="http://www.solidworks.com/sw2003/schema" swObjCount="3" swVersion="18000"><swHeader swObjCount="2"><swFile id="3" swDocType="PART" swCreationTime="1706104060" swPath="C:\work\вал.SLDPRT"/><swFile id="5" swDocType="PART" swCreationTime="956348152" swPath="C:\lib\заготовка.sldprt"/></swHeader><swModelList swObjCount="2"><swModel id="2" swName="вал" swConfigurationName="По умолчанию" swConfigurationId="0" swLastModifiedStamp="8316" swFileRef="3"/><swModel id="6" swName="заготовка" swConfigurationName="PreviewCfg" swConfigurationId="19" swFileRef="5"/></swModelList><swConfigurationList swObjCount="1"><swConfiguration id="1" swName="По умолчанию" swID="0" swReference="Вал" swMostRecentConfiguration="YES" swConfigurationNeedsUpdate="{needs_update}" swDefeatureConfiguration="NO" swModelRef="2"/></swConfigurationList><swExtFeatureList>{external}</swExtFeatureList></swSolidWorks>"#
    )
}

const INFORMATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/SolidworksInformation-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><propertySection xmlns="" name="UserDefinedProperties" fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}"><property name="SW-Folder Name" pid="4"><vt:lpstr>C:\work\</vt:lpstr></property><property name="SW-Last Saved By" pid="14"><vt:lpstr>Секретов</vt:lpstr></property><property name="SW-Author" pid="7"><vt:lpstr>Секретов</vt:lpstr></property><property name="SW-HasDesignTable" pid="16"><vt:i4>0</vt:i4></property><property name="SW_UnitsLinear" pid="30"><vt:i4>0</vt:i4></property><property name="SW_UnitsLinearDecimalPlaces" pid="31"><vt:i4>2</vt:i4></property><property name="SW-BOM Part Number" pid="40"><vt:lpstr>Вал</vt:lpstr></property><property name="SW-MassProp-Config-0" pid="41"><vt:lpstr>0.08, 0.05, -0.01, 0.000436, 0.1628, 1.2131691352424556, 0.1, 0.2, 0.3, 0, 0, 0, 1.000000, 0.000000</vt:lpstr></property></propertySection></Properties>"#;

const CONFIG: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<ConfigProperties xmlns="http://www.solidworks.com/config-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><propertySection xmlns="" name="DocumentSummaryInformation" fmtid="{D5CDD502-2E9C-101B-9397-08002B2CF9AE}"><property name="" pid="1" TypeID="0"><vt:i2>65001</vt:i2></property></propertySection><propertySection xmlns="" name="UserDefinedProperties" fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}"><property name="" pid="1" TypeID="0"><vt:i2>65001</vt:i2></property><property name="Обозначение" pid="3" IsEquation="False" TypeID="30"><vt:lpstr>АБВГ.715423.001</vt:lpstr></property><property name="Наименование" pid="4" IsEquation="False" TypeID="30"><vt:lpstr></vt:lpstr></property><property name="Масса" pid="5" IsEquation="False" TypeID="30"><vt:lpstr>1.21</vt:lpstr><FPVals><vt:lpstr>&lt;$5$1.2131691352424556$&gt;</vt:lpstr></FPVals></property><property name="" pid="16777221" IsEquation="False" TypeID="30"><vt:lpstr>"SW-Mass@@По умолчанию@вал.SLDPRT"</vt:lpstr></property><property name="Раздел" pid="7" IsEquation="False" TypeID="30"><vt:lpstr>Детали</vt:lpstr><FPVals><vt:lpstr>Детали</vt:lpstr></FPVals></property></propertySection></ConfigProperties>"#;

const CUSTOM: &str = r#"<?xml version="1.0"?><Properties xmlns:vt="x"><propertySection xmlns="" name="UserDefinedProperties"><property name="Заказ" pid="2"><vt:lpstr>17</vt:lpstr></property></propertySection></Properties>"#;

/// Секция `KeyWords`, как в файле: со служебным байтом перед `<?xml`.
fn keywords(xml: &str) -> Vec<u8> {
    [&[0x86][..], xml.as_bytes()].concat()
}

fn part(keywords_xml: &[u8], features_xml: &str) -> Container {
    let bytes = Container::pack(&[
        ("swXmlContents/KeyWords", keywords_xml),
        ("swXmlContents/Features", features_xml.as_bytes()),
        (
            "docProps/ISolidWorksInformation.xml",
            INFORMATION.as_bytes(),
        ),
        ("docProps/Config-0-Properties.xml", CONFIG.as_bytes()),
        ("docProps/custom.xml", CUSTOM.as_bytes()),
    ]);
    Container::from_bytes(bytes).unwrap()
}

#[test]
fn part_design_is_read_whole() {
    let design = Design::read(&part(&keywords(KEYWORDS), &features("NO", "")));
    assert_eq!(design.gaps, []);
    assert_eq!(design.kind, Some(DocKind::Part));
    assert_eq!(design.doubts(), []);

    let tree = design.tree.as_ref().unwrap();
    assert_eq!(tree.root_id, Some(1_706_104_060));
    assert_eq!(tree.root_name, "Вал");
    // По возрастанию номера, а не как в файле; узлы «-1» пропущены.
    let ids: Vec<i64> = tree.operations.iter().map(|o| o.id).collect();
    assert_eq!(ids, [1, 2, 5, 34, 53, 610, 1910, 2127, 2130]);
    assert_eq!(tree.skipped, 2);
    let real: Vec<&str> = tree
        .operations
        .iter()
        .filter(|o| !o.service)
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(
        real,
        [
            "Бобышка-Вытянуть1",
            "Эскиз28",
            "Бобышка-Вытянуть2",
            "Скругление20",
            "Фаска1"
        ]
    );
    let boss = tree.operation(1910).unwrap();
    assert_eq!((boss.tag, boss.kind.as_str()), (Tag::Extrusion, ""));
    assert_eq!(boss.children, [610]);
    let fillet = tree.operation(2127).unwrap();
    assert_eq!(fillet.kind, "Скругление");
    assert_eq!(fillet.dimensions[1].value, Some(14.5));
    let marks: Vec<(Mark, Option<f64>)> = tree
        .operation(2130)
        .unwrap()
        .dimensions
        .iter()
        .map(|d| (d.mark, d.value))
        .collect();
    assert_eq!(
        marks,
        [
            (Mark::Radius, Some(2.0)),
            (Mark::Angle, Some(45.0)),
            (Mark::Text, None)
        ]
    );
    let sketch = &tree.operation(610).unwrap().dimensions[0];
    assert_eq!((sketch.mark, sketch.value), (Mark::Diameter, Some(42.0)));

    let manifest = design.manifest.as_ref().unwrap();
    assert_eq!(manifest.sw_version, Some(18000));
    assert_eq!(manifest.documents[0].created, tree.root_id);
    assert_eq!(manifest.modified_stamp, Some(8316));
    let active = manifest.active().unwrap();
    assert_eq!((active.name.as_str(), active.id), ("По умолчанию", Some(0)));
    assert!(!active.needs_update);

    assert_eq!(design.active_config(), Some(0));
    assert_eq!(design.material(), Some("2024-T3"));
    assert_eq!(
        design.saved_mass().unwrap().mass_kg,
        1.213_169_135_242_455_6
    );
    assert_eq!(design.props.units_linear, Some(0));
    assert_eq!(design.props.decimals, Some(2));
    assert_eq!(design.props.design_table, Some(false));
    assert_eq!(design.props.bom_number.as_deref(), Some("Вал"));
    let mass = design.props.property(0, "Масса").unwrap();
    assert_eq!(mass.shown, "1.21");
    assert_eq!(mass.exact.as_deref(), Some("1.2131691352424556"));
    assert_eq!(
        design.props.property(0, "Обозначение").unwrap().shown,
        "АБВГ.715423.001"
    );
    // Пустое свойство и безымянная формула свойством не считаются.
    assert_eq!(design.props.configurations[&0].len(), 3);
    assert_eq!(design.props.property(0, "Заказ").unwrap().shown, "17");
}

/// Имя учётной записи автора из файла не выходит: ни в данные, ни в
/// отладочный вывод.
#[test]
fn personal_data_stays_in_the_file() {
    let design = Design::read(&part(&keywords(KEYWORDS), &features("NO", "")));
    assert!(!format!("{design:?}").contains("Секретов"));
    let all: Vec<String> = design.gaps.iter().map(ToString::to_string).collect();
    assert!(!all.concat().contains("Секретов"));
}

#[test]
fn doubtful_part_says_why() {
    let external = r#"<swExtFeature id="4" swName="заготовка" swFeatureType="Stock" swConfigurationName="По умолчанию" swSuppressed="0" swModelRef="6" swID="59"/><swExtFeature id="7" swName="Эскиз3" swFeatureType="ProfileFeature" swModelRef="6" swID="61"/>"#;
    let design = Design::read(&part(&keywords(KEYWORDS), &features("YES", external)));
    assert_eq!(
        design.doubts(),
        [
            Doubt::NeedsRebuild,
            Doubt::Derived,
            Doubt::ExternalReferences(1)
        ]
    );
    let stock = &design.manifest.as_ref().unwrap().external[0];
    assert_eq!(stock.document.as_deref(), Some(r"C:\lib\заготовка.sldprt"));
    assert_eq!(stock.id, Some(59));
    let codes: Vec<&str> = design.doubts().iter().map(Doubt::code).collect();
    assert_eq!(codes, ["needs_rebuild", "derived", "external_references"]);
    assert!(Doubt::Derived.to_string().contains("производная"));
}

#[test]
fn garbage_is_read_partly_and_named() {
    // Список операций оборван на полуслове: что было целым — прочитано.
    let cut = &keywords(KEYWORDS)[..KEYWORDS.find("Скругление20").unwrap()];
    let design = Design::read(&part(cut, &features("NO", "")));
    let tree = design.tree.as_ref().unwrap();
    assert!(tree.operation(1910).is_some() && tree.operation(2127).is_none());
    assert_eq!(design.gaps.len(), 1);
    assert_eq!(
        (design.gaps[0].piece, design.gaps[0].why),
        (Piece::Tree, Why::Garbled)
    );
    assert_eq!(design.gaps[0].code(), "tree.garbled");
    assert_eq!(design.doubts(), [Doubt::NotWhole]);

    // Вместо XML — случайные байты; свойств конфигурации нет вовсе.
    let bytes = Container::pack(&[
        ("swXmlContents/KeyWords", &[0x86, 1, 2, 3, 4, 5, 6, 7, 8, 9][..]),
        ("swXmlContents/Features", b"<other/>"),
        ("docProps/ISolidWorksInformation.xml", b"<Properties><propertySection><property name=\"SW_UnitsLinear\"><vt:i4>x</vt:i4></property>"),
    ]);
    let design = Design::read(&Container::from_bytes(bytes).unwrap());
    let codes: Vec<String> = design.gaps.iter().map(|g| g.code()).collect();
    assert_eq!(
        codes,
        [
            "tree.not_xml",
            "manifest.not_xml",
            "information.garbled",
            "document_props.missing"
        ]
    );
    assert_eq!(design.props.units_linear, None);
    assert!(design.gaps[0].to_string().contains("список операций"));
}

const ASSEMBLY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<swSolidWorks xmlns="http://www.solidworks.com/sw2003/schema" swVersion="16000"><swHeader><swFile id="3" swDocType="ASSEMBLY" swCreationTime="1727275623" swPath="C:\work\стол.SLDASM"/><swFile id="4" swDocType="PART" swPath="C:\work\рельса.SLDPRT"/><swFile id="5" swDocType="ASSEMBLY" swPath="C:\work\узел.SLDASM"/><swFile id="6" swDocType="PART" swPath="C:\Temp\swx1\VC~~\Деталь1^стол.SLDPRT"/><swFile id="7" swDocType="PART" swPath="C:\work\гайка.stp.SLDPRT" swImportedPath="C:\in\гайка.stp"/></swHeader><swModelList><swModel id="11" swName="рельса" swFileRef="4"/><swModel id="12" swName="стол" swFileRef="5"><swReference id="20" swName="рельса" swReferenceNumber="7" swID="9" swModelRef="11" swConfigurationName="По умолчанию" swSuppressed="NO"/></swModel><swModel id="13" swName="Деталь1^стол" swFileRef="6"/><swModel id="14" swName="гайка" swFileRef="7"/><swModel id="2" swName="стол" swConfigurationName="По умолчанию" swLastModifiedStamp="566" swFileRef="3"><swReference id="7" swName="рельса" swReferenceNumber="1" swID="29" swIsVirtualComponent="NO" swConfigurationName="По умолчанию" swSuppressed="NO" swHidden="NO" swExcludeFromBOM="NO" swModelRef="11" swTransform="0 0 1 0 0 1 0 0 -1 0 0 0 0.72 0.0205 0.7 1"/><swReference id="8" swName="рельса" swReferenceNumber="2" swID="41" swSuppressed="YES" swHidden="YES" swExcludeFromBOM="YES" swModelRef="11" swTransform="1 0 0"/><swReference id="9" swName="узел" swReferenceNumber="1" swID="50" swModelRef="12"/><swReference id="10" swName="Деталь1^стол" swReferenceNumber="1" swID="51" swIsVirtualComponent="YES" swModelRef="13"/><swReference id="15" swName="гайка" swReferenceNumber="1" swID="52" swModelRef="14"/></swModel></swModelList><swConfigurationList><swConfiguration id="1" swName="По умолчанию" swID="0" swMostRecentConfiguration="YES" swConfigurationNeedsUpdate="NO" swModelRef="2"/></swConfigurationList><swExtFeatureList swObjCount="0"/></swSolidWorks>"#;

#[test]
fn assembly_lists_its_components() {
    let bytes = Container::pack(&[
        (
            "swXmlContents/KeyWords",
            &keywords(
                r#"<Keywords id="1727275623" Name="стол"><Reference id="29" Name="рельса-1" Type="рельса-"/></Keywords>"#,
            )[..],
        ),
        ("swXmlContents/COMPINSTANCETREE", ASSEMBLY.as_bytes()),
        ("docProps/ISolidWorksInformation.xml", b"<Properties/>"),
        ("docProps/custom.xml", b"<Properties/>"),
    ]);
    let design = Design::read(&Container::from_bytes(bytes).unwrap());
    assert_eq!(design.gaps, []);
    assert_eq!(design.kind, Some(DocKind::Assembly));
    let manifest = design.manifest.as_ref().unwrap();
    let names: Vec<(&str, usize)> = manifest
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.depth))
        .collect();
    assert_eq!(
        names,
        [
            ("рельса-1", 0),
            ("рельса-2", 0),
            ("узел-1", 0),
            ("рельса-7", 1),
            ("Деталь1^стол-1", 0),
            ("гайка-1", 0)
        ]
    );
    let first = &manifest.components[0];
    assert_eq!(first.id, Some(29));
    assert_eq!(first.path.as_deref(), Some(r"C:\work\рельса.SLDPRT"));
    assert_eq!(first.transform.unwrap()[12..15], [0.72, 0.0205, 0.7]);
    let second = &manifest.components[1];
    assert!(second.suppressed && second.hidden && second.out_of_bom);
    assert_eq!(second.transform, None);
    assert!(manifest.components[4].virtual_part && manifest.components[5].imported);
    // Напрямую — без повторов, без виртуальных и импортированных.
    assert_eq!(
        manifest.direct_files(),
        [r"C:\work\рельса.SLDPRT", r"C:\work\узел.SLDASM"]
    );
    // Экземпляр верхнего уровня виден и в списке операций, с тем же номером.
    assert_eq!(
        design.tree.unwrap().operation(29).unwrap().tag,
        Tag::Reference
    );
}

#[test]
fn drawing_names_its_model() {
    let information = r#"<Properties xmlns:vt="x"><propertySection name="UserDefinedProperties"><property name="SW-Config-Model-View-Sheet-List" pid="53"><vt:lpstr>2:@@По умолчанию@0@C:\work\вал.SLDPRT@Чертежный вид1@Лист1@По умолчанию@0@C:\work\вал.SLDPRT@Разрез A-A@Лист1</vt:lpstr></property></propertySection></Properties>"#;
    let bytes = Container::pack(&[
        (
            "swXmlContents/KeyWords",
            &keywords(
                r#"<Keywords id="954957803" Name="вал"><Sheet id="2" Name="Лист1" Type="Лист"><View Name="Чертежный вид1">вал.sldprt</View></Sheet></Keywords>"#,
            )[..],
        ),
        (
            "docProps/ISolidWorksInformation.xml",
            information.as_bytes(),
        ),
        ("docProps/custom.xml", b"<Properties/>"),
        ("DWG/DWG_HEADER", b"x"),
    ]);
    let design = Design::read(&Container::from_bytes(bytes).unwrap());
    assert_eq!(design.gaps, []);
    assert_eq!(design.kind, Some(DocKind::Drawing));
    assert_eq!(design.manifest, None);
    let models = &design.props.drawing_models;
    assert_eq!(models.len(), 2);
    assert_eq!(models[1].path, r"C:\work\вал.SLDPRT");
    assert_eq!(models[1].view, "Разрез A-A");
    assert_eq!(design.doubts(), []);
}

/// Модуль точки `DesignSource`: контракт и то, что видит ядро.
#[test]
fn module_passes_contract_and_speaks_core_types() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("вал.SLDPRT");
    let external = r#"<swExtFeature id="4" swName="заготовка" swFeatureType="Stock" swModelRef="6" swID="59"/>"#;
    let bytes = Container::pack(&[
        ("swXmlContents/KeyWords", &keywords(KEYWORDS)[..]),
        (
            "swXmlContents/Features",
            features("NO", external).as_bytes(),
        ),
        (
            "docProps/ISolidWorksInformation.xml",
            INFORMATION.as_bytes(),
        ),
        ("docProps/Config-0-Properties.xml", CONFIG.as_bytes()),
    ]);
    std::fs::write(&file, bytes).unwrap();
    let module = SldprtDesign::new();
    if let Err(v) = contract::design_source(&module, &file) {
        panic!("нарушения контракта:\n- {}", v.join("\n- "));
    }
    assert!(module.can_read(std::path::Path::new("сборка.sldasm")));
    assert!(!module.can_read(std::path::Path::new("вал.step")));

    let read = module.read_design(&file).unwrap();
    let codes: Vec<&str> = read.gaps.iter().map(|g| g.code.as_str()).collect();
    assert_eq!(codes, ["document_props.missing"]);
    let design = read.value;
    let fillet = design.operation("2127").unwrap();
    assert_eq!(
        (fillet.name.as_str(), fillet.kind.as_str()),
        ("Скругление20", "Скругление")
    );
    assert_eq!(fillet.sizes["D1"].shown, "8");
    assert_eq!(fillet.sizes["D1"].value, Some(8.0));
    // Тип не записан — берётся по тегу.
    assert_eq!(design.operation("1910").unwrap().kind, "Extrusion");
    assert!(design.operation("1").unwrap().service);
    assert_eq!(design.configurations, ["По умолчанию"]);
    assert_eq!(design.active_configuration.as_deref(), Some("По умолчанию"));
    assert_eq!(design.material.as_deref(), Some("2024-T3"));
    assert_eq!(design.saved_mass_kg, Some(1.213_169_135_242_455_6));
    assert_eq!(design.properties["Масса"], "1.21");
    let doubts: Vec<&str> = design.doubts.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(doubts, ["derived", "not_whole"]);

    // Не файл SolidWorks — ошибка, а не пустой замысел.
    let junk = tmp.path().join("мусор.sldprt");
    std::fs::write(&junk, b"not a part").unwrap();
    assert!(module.read_design(&junk).is_err());

    let mut registry = Registry::new();
    nonius_sldprt::register(&mut registry).unwrap();
    assert!(registry.design_source_for(&file).is_some());
    assert!(registry.glance_for(&file).is_some());
}
