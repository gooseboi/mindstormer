use anyhow::{bail, ensure, Context};
use quick_xml::{
    events::{BytesDecl, Event},
    reader::Reader,
};
use std::cell::RefCell;
use std::fs::File;
use std::io::Read;
use std::rc::Rc;
use std::str;

mod utils;
mod xml_utils;

use utils::VecReadWrapper;
use xml_utils::{
    collect_to_vec, extract_name_from_qname, parse_attributes, ParsedAttribute, XMLReader,
};

#[allow(unused)]
fn dump_tag(name: String, prefix: Option<String>, attributes: Vec<ParsedAttribute>) {
    println!("Dumping tag");
    println!("Name: {name}");
    println!("Prefix: {prefix:?}");
    match attributes.len() {
        0 => println!("    No attributes"),
        n => {
            for attr in attributes {
                println!("    Attr: {attr:?}");
            }
        }
    }
    println!();
}

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
            let _ = &f.root;
        }
        bail!("Outputting the project not yet implemented")
    }
}

#[derive(Clone, Default, Debug)]
struct Version {
    number: String,
    namespace: String,
}

struct EV3File {
    decl: BytesDecl<'static>,
    version: Version,
    name: String,
    root: EV3Block,
}

impl EV3File {
    fn new(name: &str, contents: Vec<u8>) -> anyhow::Result<Self> {
        let wrapper = VecReadWrapper::new(contents);
        let mut xml = Reader::from_reader(wrapper);
        xml.trim_text(true);
        let mut builder = EV3FileBuilder::from_xml(xml)?;
        builder.name(name.into())?;
        builder.parse().context("Failed parsing file contents")?;
        builder.build().context("Failed building file struct")
    }
}

struct EV3FileBuilder {
    decl: Option<BytesDecl<'static>>,
    version: Option<Version>,
    name: Option<String>,
    root: Option<EV3Block>,
    events: Vec<Event<'static>>,
    idx: usize,
}

enum SequenceBlockType {
    In,
    Out,
}

struct SequenceBlock {
    ty: SequenceBlockType,
    bounds: (usize, usize),
}

enum EV3BlockType {
    Start,
}

struct EV3Block {
    ty: EV3BlockType,
    id: String,
    bounds: (usize, usize),
    // TODO: Should this be children?
    child: Option<Rc<RefCell<EV3Block>>>,
    sequence_in: Option<SequenceBlock>,
    sequence_out: Option<SequenceBlock>,
}

impl EV3FileBuilder {
    fn from_xml(xml: XMLReader) -> anyhow::Result<Self> {
        let name = Default::default();
        let version = Default::default();
        let decl = Default::default();
        let root = Default::default();
        let events = collect_to_vec(xml).context("Failed parsing XML file")?;
        Ok(Self {
            events,
            idx: 0,
            name,
            version,
            decl,
            root,
        })
    }

    fn next_event(&mut self) -> anyhow::Result<Event<'static>> {
        ensure!(
            self.events.len() > self.idx,
            "Invalid index {} into events of length {}",
            self.idx,
            self.events.len()
        );
        let event = self.events[self.idx].clone();
        self.idx += 1;
        Ok(event)
    }

    fn peek_event(&self) -> anyhow::Result<Event<'static>> {
        ensure!(
            self.events.len() > self.idx,
            "Invalid index {} into events of length {}",
            self.idx,
            self.events.len()
        );
        Ok(self.events[self.idx].clone())
    }

    fn parse(&mut self) -> anyhow::Result<()> {
        loop {
            match self.next_event()? {
                Event::Start(t) => {
                    let qname = t.name();
                    let (name, prefix) =
                        extract_name_from_qname(qname).context("Failed parsing start tag name")?;
                    let attributes =
                        parse_attributes(&t).context("Failed parsing start tag attributes")?;

                    self.parse_start_tag(name, prefix, attributes)?;
                }
                Event::End(t) => {
                    let qname = t.name();
                    let (name, prefix) =
                        extract_name_from_qname(qname).context("Failed parsing end tag name")?;

                    self.parse_end_tag(name, prefix)?;
                }
                Event::Empty(t) => {
                    let qname = t.name();
                    let (name, prefix) =
                        extract_name_from_qname(qname).context("Failed parsing start tag name")?;
                    let attributes =
                        parse_attributes(&t).context("Failed parsing start tag attributes")?;

                    self.parse_empty_tag(name, prefix, attributes)?;
                }
                Event::Text(t) => {
                    let s = t.clone().into_inner().to_mut().to_vec();
                    let s = String::from_utf8(s).unwrap();
                    println!("TODO: Text tag: {}", s);
                }
                Event::Comment(_) => println!("Ignoring Comment"),
                Event::CData(_) => println!("Found CData"),
                Event::Decl(d) => self.decl(d.clone().into_owned())?,
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
                if let Some(prefix) = prefix {
                    bail!("Unexpected prefix namespace {prefix} in `SourceFile` start tag");
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
                self.version(Version { number, namespace })?;
            }
            "Namespace" => {
                if let Some(prefix) = prefix {
                    bail!("Unexpected prefix namespace {prefix} in `Namespace` start tag");
                }
                for attr in attributes {
                    if attr.key.0 == "Name" && attr.value != "Project" {
                        bail!("Unsupported namespace {} that is not project", attr.value);
                    }
                }
            }
            // TODO: Should this do something?
            "VirtualInstrument" => {}
            // TODO: Should this do something?
            "FrontPanel" => {}
            "BlockDiagram" => {
                for attr in attributes {
                    ensure!(
                        attr.key.0 == "Name",
                        "Unknown block diagram attribute {}",
                        attr.key.0
                    );
                    ensure!(
                        attr.value == "__RootDiagram__",
                        "Unknown block diagram name value {}",
                        attr.value
                    );
                }
            }
            "StartBlock" => {
                if let Some(prefix) = prefix {
                    bail!("Unexpected prefix namespace `{prefix}` in `StartBlock` start tag");
                }
                let mut id = None;
                let mut width = None;
                let mut height = None;
                for attr in attributes {
                    match attr.key.0.as_str() {
                        "Id" => id = Some(attr.value),
                        // Ignore, because we already know it's a start block
                        "Target" => {}
                        "Bounds" => {
                            let (w, h) = parse_bounds(attr.value)
                                .context("Failed parsing bounds for `StartBlock`")?;
                            width = Some(w);
                            height = Some(h);
                        }
                        _ => bail!("Unknown attribute for `{name}`: {}", attr.value),
                    }
                }
                let id = id.context("Missing id for StartBlock")?;
                let width = width.context("Missing width for StartBlock")?;
                let height = height.context("Missing height for StartBlock")?;

                let event = self.next_event()?;
                let Event::Start(t) = event else {
                    bail!("Expected start tag in StartBlock");
                };
                let qname = t.name();
                let (name, prefix) = extract_name_from_qname(qname)
                    .context("Failed parsing start tag name in StartBlock")?;
                let attributes = parse_attributes(&t)
                    .context("Failed parsing start tag attributes in StartBlock")?;
                ensure!(
                    attributes.is_empty(),
                    "Unexpected attributes in StartBlock tag"
                );
                if let Some(prefix) = prefix {
                    bail!("Unexpected prefix `{prefix}` in StartBlock tag");
                }
                ensure!(
                    name == "ConfigurableMethodTerminal",
                    "Unexpected tag name `{name}` in StartBlock tag"
                );

                self.root = Some(EV3Block {
                    ty: EV3BlockType::Start,
                    id,
                    bounds: (width, height),
                    child: None,
                    sequence_in: None,
                    sequence_out: bail!("sequence out unimplemented"),
                });
            }
            _ => {
                dump_tag(name.clone(), prefix, attributes);
                bail!("{name} start tag not implemented");
            }
        }
        Ok(())
    }

    fn parse_end_tag(&mut self, name: String, prefix: Option<String>) -> anyhow::Result<()> {
        if let Some(prefix) = prefix {
            bail!("Prefix `{prefix}` present in `{name}` end tag");
        }
        match name.as_str() {
            // Same as line 186
            "FrontPanel" => {}
            _ => bail!("{name} end tag not implemented"),
        }
        Ok(())
    }

    fn parse_empty_tag(
        &mut self,
        name: String,
        prefix: Option<String>,
        attributes: Vec<ParsedAttribute>,
    ) -> anyhow::Result<()> {
        let _ = prefix;
        let _ = attributes;
        match name.as_str() {
            // TODO: Should this do something?
            "FrontPanelCanvas" => {}
            _ => bail!("{name} empty tag not implemented"),
        }
        Ok(())
    }

    fn name(&mut self, name: String) -> anyhow::Result<()> {
        if self.name.is_some() {
            bail!("Setting builder name twice");
        }
        self.name = Some(name);
        Ok(())
    }

    fn version(&mut self, version: Version) -> anyhow::Result<()> {
        if self.version.is_some() {
            bail!(
                "Setting builder version twice. Old {:?}, new {:?}",
                self.version.clone().unwrap(),
                version
            );
        }
        self.version = Some(version);
        Ok(())
    }

    fn decl(&mut self, decl: BytesDecl<'static>) -> anyhow::Result<()> {
        if self.decl.is_some() {
            bail!(
                "Setting builder decl twice. Old {:?}, new {:?}",
                self.decl.clone().unwrap(),
                decl
            );
        }
        self.decl = Some(decl);
        Ok(())
    }

    fn build(self) -> anyhow::Result<EV3File> {
        let name = self.name.context("No name found")?;
        let version = self.version.context("No version found")?;
        let decl = self.decl.context("No decl found")?;
        let root = self.root.context("No root found")?;
        Ok(EV3File {
            name,
            version,
            decl,
            root,
        })
    }
}

fn parse_bounds(input: String) -> anyhow::Result<(usize, usize)> {
    let vals: anyhow::Result<Vec<usize>> = input
        .split(' ')
        .map(|n| n.parse().context("Invalid number in bounds"))
        .collect();
    let vals = vals?;
    if vals.len() > 4 {
        bail!("Too many bounds");
    } else if vals.len() < 4 {
        bail!("Too little bounds");
    }
    let width = vals[2];
    let height = vals[3];
    Ok((width, height))
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
