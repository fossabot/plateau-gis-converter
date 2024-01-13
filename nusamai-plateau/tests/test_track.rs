use std::io::BufRead;
use std::path::Path;

use url::Url;

use nusamai_citygml::{
    CityGMLElement, CityGMLReader, Code, GeometryStore, ParseError, SubTreeReader,
};
use nusamai_plateau::models::TopLevelCityObject;
use nusamai_plateau::models::Track;

#[derive(Default, Debug)]
struct ParsedData {
    tracks: Vec<Track>,
    geometries: Vec<GeometryStore>,
}

fn toplevel_dispatcher<R: BufRead>(st: &mut SubTreeReader<R>) -> Result<ParsedData, ParseError> {
    let mut parsed_data = ParsedData::default();

    match st.parse_children(|st| match st.current_path() {
        b"core:cityObjectMember" => {
            let mut cityobj: TopLevelCityObject = Default::default();
            cityobj.parse(st)?;
            match cityobj {
                TopLevelCityObject::Track(trk) => {
                    parsed_data.tracks.push(trk);
                }
                TopLevelCityObject::CityObjectGroup(_) => (),
                e => panic!("Unexpected city object type: {:?}", e),
            }
            let geometries = st.collect_geometries();
            parsed_data.geometries.push(geometries);
            Ok(())
        }
        b"gml:boundedBy" | b"app:appearanceMember" => {
            st.skip_current_element()?;
            Ok(())
        }
        other => Err(ParseError::SchemaViolation(format!(
            "Unrecognized element {}",
            String::from_utf8_lossy(other)
        ))),
    }) {
        Ok(_) => Ok(parsed_data),
        Err(e) => {
            println!("Err: {:?}", e);
            Err(e)
        }
    }
}

#[test]
fn test_track() {
    let filename = "./tests/data/plateau-3_0/udx/trk/53361601_trk_6697.gml";

    let reader = std::io::BufReader::new(std::fs::File::open(filename).unwrap());
    let mut xml_reader = quick_xml::NsReader::from_reader(reader);

    let code_resolver = nusamai_plateau::codelist::Resolver::new();
    let source_url =
        Url::from_file_path(std::fs::canonicalize(Path::new(filename)).unwrap()).unwrap();
    let context = nusamai_citygml::ParseContext::new(source_url, &code_resolver);

    let parsed_data = match CityGMLReader::new(context).start_root(&mut xml_reader) {
        Ok(mut st) => match toplevel_dispatcher(&mut st) {
            Ok(parsed_data) => parsed_data,
            Err(e) => panic!("Err: {:?}", e),
        },
        Err(e) => panic!("Err: {:?}", e),
    };

    assert_eq!(parsed_data.tracks.len(), 125);
    assert_eq!(parsed_data.tracks.len(), parsed_data.geometries.len());

    let track = parsed_data.tracks.first().unwrap();

    assert_eq!(track.function, vec![Code::new("徒歩道".into(), "1".into())]);

    assert_eq!(
        track
            .tran_data_quality_attribute
            .as_ref()
            .unwrap()
            .geometry_src_desc,
        vec![Code::new("既成図数値化".into(), "6".into())]
    );

    assert_eq!(
        track.auxiliary_traffic_area.first().unwrap().function,
        vec![Code::new("島".into(), "3000".into())]
    );

    assert_eq!(
        track.track_attribute.first().unwrap().admin_type,
        Some(Code::new("市区町村".into(), "3".into()))
    );
}
