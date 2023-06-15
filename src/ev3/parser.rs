use super::project::{EV3File, Version};
use crate::utils::xml::{
    collect_to_vec, extract_name_from_qname, parse_attributes, ParsedAttribute, XMLReader,
};
use anyhow::{bail, ensure, Context};
use quick_xml::events::{BytesDecl, Event};
use std::cell::RefCell;
use std::rc::Rc;

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

#[derive(Default)]
pub struct EV3FileBuilder {
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
    wire_id: String,
}

enum EV3BlockType {
    Start,
}

pub struct EV3Block {
    ty: EV3BlockType,
    id: String,
    bounds: (usize, usize),
    // TODO: Should this be children?
    child: Option<Rc<RefCell<EV3Block>>>,
    sequence_in: Option<SequenceBlock>,
    sequence_out: Option<SequenceBlock>,
}

impl EV3FileBuilder {
    pub fn from_xml(xml: XMLReader) -> anyhow::Result<Self> {
        let events = collect_to_vec(xml).context("Failed parsing XML file")?;
        Ok(Self {
            events,
            idx: 0,
            ..Default::default()
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

    pub fn parse(&mut self) -> anyhow::Result<()> {
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
                    bail!("Unexpected Text tag: {}", s);
                }
                Event::Comment(_) => println!("Ignoring Comment"),
                Event::CData(_) => bail!("Unexpected CData tag"),
                Event::Decl(d) => self.decl(d.clone().into_owned())?,
                Event::PI(_) => bail!("Unexpected Processing tag"),
                Event::DocType(_) => bail!("Unexpected DocType tag"),
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
                let block = self
                    .parse_start_block(name, prefix, attributes)
                    .context("Failed parsing start block");
            }
            _ => {
                dump_tag(name.clone(), prefix, attributes);
                bail!("{name} start tag not implemented");
            }
        }
        Ok(())
    }

    fn parse_start_block(
        &mut self,
        name: String,
        prefix: Option<String>,
        attributes: Vec<ParsedAttribute>,
    ) -> anyhow::Result<EV3Block> {
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
        let attributes =
            parse_attributes(&t).context("Failed parsing start tag attributes in StartBlock")?;
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

        let Event::Empty(t) = self.next_event()? else {
                    bail!("Expected empty tag inside ConfigurableMethodTerminal in StartBlock tag");
                };
        // Ignore it cuz I assume it's always the same
        let _ = t;

        let Event::End(t) = self.next_event()? else {
                    bail!("Expected end tag to end ConfigurableMethodTerminal in StartBlock tag");
                };
        let qname = t.name();
        let (name, prefix) = extract_name_from_qname(qname)
            .context("Failed parsing start tag name in StartBlock")?;
        if let Some(prefix) = prefix {
            bail!("Unexpected prefix `{prefix}` to end ConfigurableMethodTerminal tag");
        }
        ensure!(
            name == "ConfigurableMethodTerminal",
            "Unexpected tag name `{name}` to end ConfigurableMethodTerminal"
        );

        let Event::Empty(t) = self.next_event()? else {
                    bail!("Expected empty tag in `StartBlock`");
                };
        let qname = t.name();
        let (name, prefix) = extract_name_from_qname(qname)
            .context("Failed parsing start tag name in StartBlock")?;
        let attributes =
            parse_attributes(&t).context("Failed parsing start tag attributes in StartBlock")?;
        ensure!(name == "Terminal", "Unexpected empty tag in `StartBlock`");
        if let Some(prefix) = prefix {
            bail!("Unexpected prefix `{prefix}` in empty tag");
        }

        let mut bounds = None;
        let mut wire_id = None;
        for attr in attributes {
            match attr.key.0.as_str() {
                "Id" => ensure!(
                    attr.value == "SequenceOut",
                    "Unexpected Id `{}` in `StartBlock` SequenceOut",
                    attr.value
                ),
                "Direction" => ensure!(
                    attr.value == "Output",
                    "Unexpected Direction `{}` in `StartBlock` SequenceOut",
                    attr.value
                ),
                "Wire" => wire_id = Some(attr.value),
                // TODO: Should reuse this later
                "DataType" => ensure!(
                    attr.value
                        == "NationalInstruments:SourceModel:DataTypes:X3SequenceWireDataType",
                    "Unexpected DataType `{}` in `StartBlock` SequenceOut",
                    attr.value
                ),
                // TODO: What even is this?
                "Hotspot" => {}
                "Bounds" => {
                    bounds = Some(
                        parse_bounds(attr.value)
                            .context("Failed parsing bounds in `StartBlock` SequenceOut")?,
                    );
                }
                _ => bail!(
                    "Unexpected attribute `{}` for `SequenceOut` in `StartBlock`",
                    attr.key.0
                ),
            }
        }

        let bounds = bounds.context("No bounds in `StartBlock` SequenceOut")?;
        let wire_id = wire_id.context("No wire_id in `StartBlock` SequenceOut")?;
        let sequence_out = Some(SequenceBlock {
            ty: SequenceBlockType::Out,
            bounds,
            wire_id,
        });
        let Event::End(t) = self.next_event()? else {
                    bail!("Expected end tag in `StartBlock`");
                };
        let qname = t.name();
        let (name, prefix) =
            extract_name_from_qname(qname).context("Failed parsing end tag name in StartBlock")?;
        ensure!(name == "StartBlock", "Unexpected end tag for tag `{name}`");
        if let Some(prefix) = prefix {
            bail!("Unexpected prefix `{prefix}` in end tag");
        }
        Ok(EV3Block {
            ty: EV3BlockType::Start,
            id,
            bounds: (width, height),
            child: None,
            sequence_in: None,
            sequence_out,
        })
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

    pub fn name(&mut self, name: String) -> anyhow::Result<()> {
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

    pub fn build(self) -> anyhow::Result<EV3File> {
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
    let n = vals.len();
    match n {
        4 => {
            let width = vals[2];
            let height = vals[3];
            Ok((width, height))
        }
        _ => bail!("Expected 4 bounds, found {n}"),
    }
}
