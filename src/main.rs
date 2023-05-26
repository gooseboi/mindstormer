use anyhow::{bail, Context};
use quick_xml::{
    events::{BytesDecl, Event},
    reader::Reader,
};
use std::fs::File;
use std::io::Read;
use std::str;

mod utils;
mod xml_utils;

use utils::VecReadWrapper;
use xml_utils::{extract_name_from_qname, parse_attributes, ParsedAttribute};
type XMLReader = Reader<VecReadWrapper>;

struct EV3Project {
    title: String,
    description: String,
    year: usize,
    /// Amazing, I can change the thumbnail to my project and insert naked men
    thumbnail: Vec<u8>,
    /// I don't know what this is, something or other
    activity: String,
    /// ????
    activity_assets: Vec<u8>,
    /// I assume there's no need to parse this, we don't change it
    project: String,
    files: Vec<EV3File>,
}

impl EV3Project {
    fn output_file(&self, fname: &str) -> anyhow::Result<()> {
        let _ = fname;
        let _ = &self.title;
        let _ = &self.description;
        let _ = &self.year;
        let _ = &self.thumbnail;
        let _ = &self.activity;
        let _ = &self.activity_assets;
        let _ = &self.project;
        for f in &self.files {
            let _ = &f.decl;
            let _ = &f.version.number;
            let _ = &f.version.namespace;
            let _ = &f.name;
        }
        bail!("Outputting the project not yet implemented")
    }
}

#[derive(Default)]
struct Version {
    number: String,
    namespace: String,
}

struct EV3File {
    decl: BytesDecl<'static>,
    version: Version,
    name: String,
}

impl EV3File {
    fn new(name: &str, contents: Vec<u8>) -> anyhow::Result<Self> {
        let wrapper = VecReadWrapper::new(contents);
        let mut xml = Reader::from_reader(wrapper);
        xml.trim_text(true);
        let mut builder = EV3FileBuilder::from_xml(xml);
        builder.name(name.into());
        builder.parse().context("Failed parsing file contents")?;
        builder.build().context("Failed building file struct")
    }
}

struct EV3FileBuilder {
    decl: Option<BytesDecl<'static>>,
    version: Option<Version>,
    name: Option<String>,
    xml: XMLReader,
}

impl EV3FileBuilder {
    fn from_xml(xml: XMLReader) -> Self {
        let name = Default::default();
        let version = Default::default();
        let decl = Default::default();
        Self {
            xml,
            name,
            version,
            decl,
        }
    }

    fn parse(&mut self) -> anyhow::Result<()> {
        loop {
            let mut buf = vec![];
            match self
                .xml
                .read_event_into(&mut buf)
                .context("File had invalid xml")?
            {
                Event::Start(t) => {
                    let qname = t.name();
                    let (name, prefix) =
                        extract_name_from_qname(qname).context("Failed parsing start tag name")?;
                    let attributes =
                        parse_attributes(&t).context("Failed parsing start tag attributes")?;

                    let _ = self.parse_start_tag(name, prefix, attributes)?;
                }
                Event::End(_) => println!("TODO: End tag"),
                Event::Empty(_) => println!("TODO: Empty tag"),
                Event::Text(t) => {
                    let s = t.into_inner().to_mut().iter().cloned().collect();
                    let s = String::from_utf8(s).unwrap();
                    println!("TODO: Text tag: {}", s);
                }
                Event::Comment(_) => println!("Ignoring Comment"),
                Event::CData(_) => println!("Found CData"),
                Event::Decl(d) => self.decl = Some(d.into_owned()),
                Event::PI(_) => println!("Found Processing"),
                Event::DocType(_) => println!("Found DocType"),
                Event::Eof => break,
            }
        }
        Ok(())
    }

    fn parse_start_tag(
        &mut self,
        name: String,
        prefix: Option<String>,
        attributes: Vec<ParsedAttribute>,
    ) -> anyhow::Result<()> {
        match name.as_str() {
            "SourceFile" => {
                if prefix.is_some() {
                    bail!("Unexpected prefix namespace in SourceFile start tag");
                }
                let mut number = None;
                let mut namespace = None;
                for attr in attributes {
                    match attr.key.0.as_str() {
                        "Version" => number = Some(attr.value),
                        "xmlns" => namespace = Some(attr.value),
                        _ => bail!("Unknown SourceFile attribute: {}", attr.key.0),
                    }
                }
                let number = number.context("Missing source file version number")?;
                let namespace = namespace.context("Missing source file namespace")?;
                self.version(Version { number, namespace });
            }
            _ => bail!("{name} start tag not implemented"),
        }
        Ok(())
    }

    fn name(&mut self, name: String) {
        self.name = Some(name);
    }

    fn version(&mut self, version: Version) {
        self.version = Some(version);
    }

    fn build(self) -> anyhow::Result<EV3File> {
        let name = self.name.context("No name found")?;
        let version = self.version.context("No version found")?;
        let decl = self.decl.context("No decl found")?;
        Ok(EV3File {
            name,
            version,
            decl,
        })
    }
}

fn get_project_from_zip(filename: &str) -> anyhow::Result<EV3Project> {
    let file = File::open(filename)?;
    let mut zip = zip::ZipArchive::new(file).context("Failed to read zip file")?;

    let mut title = None;
    let mut description = None;
    let mut year = None;
    let mut thumbnail = None;
    let mut activity_assets = None;
    let mut activity = None;
    let mut project = None;
    let mut files = vec![];

    for i in 0..zip.len() {
        let mut z = zip.by_index(i).context("Zip library doesn't work lol")?;

        let name = z
            .enclosed_name()
            .context("Name was invalid")?
            .to_str()
            .unwrap()
            .to_owned();

        let mut bytes = vec![];
        z.read_to_end(&mut bytes)?;

        match name.as_str() {
            "___CopyrightYear" => {
                year = Some(
                    bytes
                        .iter()
                        .fold(0, |acc, &digit| acc * 10 + (digit - 48) as usize),
                )
            }
            "___ProjectDescription" => {
                description = Some(String::from_utf8(bytes).context("Invalid description data")?)
            }
            "___ProjectTitle" => {
                title = Some(String::from_utf8(bytes).context("Invalid project title")?)
            }
            "___ProjectThumbnail" => thumbnail = Some(bytes),
            "ActivityAssets.laz" => activity_assets = Some(bytes),
            "Activity.x3a" => {
                activity = Some(String::from_utf8(bytes).context("Invalid activity(?) data")?)
            }
            "Project.lvprojx" => {
                project = Some(String::from_utf8(bytes).context("Invalid project file")?)
            }

            _ => {
                let name = name.as_str();
                files.push(EV3File::new(name, bytes).context(format!("Failed parsing {name}"))?);
            }
        }
    }
    let title = title.context("Found no title")?;
    let description = description.context("Found no description")?;
    let year = year.context("Found no year")?;
    let thumbnail = thumbnail.context("Found no thumbnail")?;
    let activity = activity.context("Found no activity")?;
    let activity_assets = activity_assets.context("Found no activity_assets")?;
    let project = project.context("Found no project")?;
    println!("Found title `{}`", title);
    println!("Found description `{}`", description);
    println!("Found year {}", year);

    Ok(EV3Project {
        title,
        description,
        year,
        thumbnail,
        activity,
        activity_assets,
        files,
        project,
    })
}

fn main() -> anyhow::Result<()> {
    let project = get_project_from_zip("1block.ev3")?;
    project.output_file("out.ev3")?;
    Ok(())
}
