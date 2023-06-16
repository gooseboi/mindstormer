use super::project::{File, Version};
use crate::utils::xml::{
    collect_to_vec, extract_name_from_qname, parse_attributes, ParsedAttribute, XMLReader,
};
use anyhow::{bail, ensure, Context};
use quick_xml::events::{BytesDecl, Event};
use std::collections::HashMap;

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

struct BlockAttribute {
    id: String,
    value: String,
}

enum SequenceBlockType {
    In,
    Out,
}

struct SequenceBlock {
    ty: SequenceBlockType,
    bounds: (usize, usize),
    wire_id: Option<String>,
}

enum BlockType {
    Start,
    MotorMove {
        ports: (char, char),
        steering: isize,
        speed: usize,
    },
}

pub struct Block {
    ty: BlockType,
    bounds: (usize, usize),
    sequence_in: Option<SequenceBlock>,
    sequence_out: Option<SequenceBlock>,
}

pub struct Wire {
    input: String,
    output: String,
}

#[derive(Default)]
pub struct FileBuilder {
    decl: Option<BytesDecl<'static>>,
    version: Option<Version>,
    name: Option<String>,
    blocks: HashMap<String, Block>,
    wires: HashMap<String, Wire>,
    events: Vec<Event<'static>>,
    idx: usize,
}

impl FileBuilder {
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
                if let Some(prefix) = prefix {
                    bail!("Unexpected prefix namespace `{prefix}` in `StartBlock` start tag");
                }
                let (id, block) = self
                    .parse_start_block(attributes)
                    .context("Failed parsing start block")?;
                // Note: Don't check for duplicates here because if two start blocks are used then
                // something bad happened
                self.blocks.insert(id, block);
            }
            "ConfigurableMethodCall" => {
                if let Some(prefix) = prefix {
                    bail!("Unexpected prefix namespace `{prefix}` in `ConfigurableMethodCall` start tag");
                }
                let (id, block) = self
                    .parse_method_call(attributes)
                    .context("Failed parsing method call")?;
                if let Some(_) = self.blocks.get(&id) {
                    bail!("Multiple blocks with id `{id}` used");
                }
                self.blocks.insert(id, block);
            }
            // I think it's safe to ignore these, as they don't really affect the program and
            // aren't changeable inside the software, so we can just reproduce them later.
            "Icon" | "IconPanel" | "AnimationProperties.Animations" | "EventProperties.Events" => {}
            _ => {
                dump_tag(name.clone(), prefix, attributes);
                bail!("{name} start tag not implemented");
            }
        }
        Ok(())
    }

    fn parse_start_block(
        &mut self,
        attributes: Vec<ParsedAttribute>,
    ) -> anyhow::Result<(String, Block)> {
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
                _ => bail!("Unknown attribute in `StartBlock`: {}", attr.value),
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
        let block = Block {
            ty: BlockType::Start,
            bounds: (width, height),
            sequence_in: None,
            sequence_out,
        };
        Ok((id, block))
    }

    fn parse_method_call(
        &mut self,
        attributes: Vec<ParsedAttribute>,
    ) -> anyhow::Result<(String, Block)> {
        let mut id = None;
        let mut bounds = None;
        let mut ty = None;
        for attr in attributes {
            let name = attr.key.0;
            match name.as_str() {
                "Id" => id = Some(attr.value),
                "Bounds" => bounds = Some(parse_bounds(attr.value)?),
                "Target" => ty = Some(attr.value),
                _ => bail!("Unexpected attribute `{name}` in `ConfigurableMethodCall`"),
            }
        }
        let id = id.context("Failed to find id for `ConfigurableMethodCall`")?;
        let bounds = bounds.context("Failed to find bounds for `ConfigurableMethodCall`")?;
        let ty = ty.context("Failed to find target type for `ConfigurableMethodCall`")?;

        let res = Ok(match ty.as_str() {
            "MoveUnlimited\\.vix" => (id, self.parse_motor_move(bounds)?),
            _ => bail!("Unknown call type {ty}"),
        });
        let Event::End(t) = self.next_event()? else {
            bail!("Expected end tag");
        };
        let qname = t.name();
        let (name, prefix) = extract_name_from_qname(qname)
            .context("Failed parsing name in ConfigurableMethodTerminal")?;
        if let Some(prefix) = prefix {
            bail!("Unexpected prefix `{prefix}` in ConfigurableMethodTerminal");
        }
        ensure!(
            name == "ConfigurableMethodCall",
            "Expected end tag for ConfigurableMethodCall, found `{name}`"
        );

        res
    }

    fn parse_motor_move(&mut self, bounds: (usize, usize)) -> anyhow::Result<Block> {
        let mut ports = None;
        let mut steering = None;
        let mut speed = None;
        while let Some(BlockAttribute { id, value }) = self
            .parse_block_attribute()
            .context("Failed parsing block attribute")?
        {
            match id.as_str() {
                "Ports" => {
                    let mut iter = value.chars();
                    iter.next();
                    iter.next();
                    let p1 = iter.next().context("Expected first port")?;
                    iter.next();
                    let p2 = iter.next().context("Expected second port")?;
                    ports = Some((p1, p2));
                }
                "Steering" => {
                    steering = Some(
                        value
                            .parse()
                            .context("Failed parsing steering value as number")?,
                    )
                }
                "Speed" => {
                    speed = Some(
                        value
                            .parse()
                            .context("Failed parsing speed value as number")?,
                    )
                }
                // Ignore cuz it's presumably always the same
                "InterruptsToListenFor_16B03592_CD76_4D58_8DC3_E3C3091E327A" => {}
                _ => bail!("Unexpected block attribute `{id}` for MotorMove"),
            }
        }
        let ports = ports.context("Failed finding ports for MotorMove")?;
        let steering = steering.context("Failed finding steering for MotorMove")?;
        let speed = speed.context("Failed finding speed for MotorMove")?;

        let (sequence_in, sequence_out) = self
            .parse_method_sequence_blocks()
            .context("Failed parsing sequence blocks for method")?;
        let sequence_in = Some(sequence_in);
        let sequence_out = Some(sequence_out);
        Ok(Block {
            bounds,
            sequence_in,
            sequence_out,
            ty: BlockType::MotorMove {
                steering,
                ports,
                speed,
            },
        })
    }

    fn parse_end_tag(&mut self, name: String, prefix: Option<String>) -> anyhow::Result<()> {
        match name.as_str() {
            // Same as line 186
            "FrontPanel" | "BlockDiagram" => {}
            // These are also safe to ignore, like the start tags
            "AnimationProperties.Animations"
            | "EventProperties.Events"
            | "IconPanel"
            | "Icon"
            | "VirtualInstrument"
            | "Namespace"
            | "SourceFile" => {}
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
            // These are hopefully safe to ignore
            "AnimationsContainer" | "EventContainer" => {}
            "Wire" => {
                let (id, wire) = self
                    .parse_wire_tag(attributes)
                    .context("Parsing wire tag failed")?;
                if let Some(_) = self.wires.get(&id) {
                    bail!("Found duplicate wire ids {id}");
                }
                self.wires.insert(id, wire);
            }
            _ => bail!("{name} empty tag not implemented"),
        }
        Ok(())
    }

    fn parse_block_attribute(&mut self) -> anyhow::Result<Option<BlockAttribute>> {
        let Event::Start(t) = self.peek_event()? else {
            return Ok(None);
        };
        // Skip it since it's what we want
        self.next_event()?;

        let qname = t.name();
        let (name, prefix) = extract_name_from_qname(qname)
            .context("Failed parsing name in ConfigurableMethodTerminal")?;
        let mut attributes = parse_attributes(&t)
            .context("Failed parsing attributes in ConfigurableMethodTerminal")?;
        if let Some(prefix) = prefix {
            bail!("Unexpected prefix `{prefix}` in ConfigurableMethodTerminal");
        }
        ensure!(
            name == "ConfigurableMethodTerminal",
            "Unexpected start tag `{name}` where ConfigurableMethodTerminal was expected"
        );
        ensure!(
            attributes.len() == 1,
            "Expected only 1 attribute in ConfigurableMethodTerminal, found {}",
            attributes.len()
        );
        let value = {
            let attr = attributes.pop().unwrap();
            let name = attr.key.0;
            ensure!(
                name == "ConfiguredValue",
                "Expected attribute ConfiguredValue, found `{name}`"
            );
            attr.value
        };
        let Event::Empty(t) = self.peek_event()? else {
            bail!("Expected empty tag after ConfigurableMethodTerminal tag");
        };
        // Same thing, we already know it so skip it
        self.next_event()?;

        let qname = t.name();
        let (name, prefix) = extract_name_from_qname(qname)
            .context("Failed parsing name in ConfigurableMethodTerminal")?;
        let attributes = parse_attributes(&t)
            .context("Failed parsing attributes in ConfigurableMethodTerminal")?;

        if let Some(prefix) = prefix {
            bail!("Unexpected prefix `{prefix}` in ConfigurableMethodTerminal");
        }
        ensure!(
            name == "Terminal",
            "Expected `Terminal` empty tag, found `{name}`"
        );
        let mut id = None;
        for attr in attributes {
            let name = attr.key.0;
            match name.as_str() {
                "Id" => id = Some(attr.value),
                "Direction" | "DataType" | "Hotspot" | "Bounds" => {}
                _ => bail!("Unexpected attribute `{name}` in Terminal"),
            }
        }
        let id = id.context("Failed to find id in Terminal")?;
        let Event::End(_) = self.next_event()? else {
            bail!("Expected ConfigurableMethodTerminal end tag, found other");
        };

        Ok(Some(BlockAttribute { id, value }))
    }

    fn parse_method_sequence_blocks(&mut self) -> anyhow::Result<(SequenceBlock, SequenceBlock)> {
        let Event::Empty(t) = self.next_event()? else {
            bail!("Expected empty tag for parsing sequence block");
        };

        let qname = t.name();
        let (name, prefix) =
            extract_name_from_qname(qname).context("Failed parsing empty tag name")?;
        let attributes = parse_attributes(&t).context("Failed parsing empty tag attributes")?;
        if let Some(prefix) = prefix {
            bail!("Unexpected prefix namespace {prefix} in `ConfigurableMethodCall` sequence tag");
        }
        ensure!(
            name == "Terminal",
            "Expected tag name Terminal, found `{name}`"
        );
        let mut wire_id = None;
        let mut bounds = None;
        for attr in attributes {
            let name = attr.key.0;
            match name.as_str() {
                "Id" => ensure!(
                    attr.value == "SequenceIn",
                    "Expected `SequenceIn` id, found `{}`",
                    attr.value
                ),
                "Direction" => ensure!(
                    attr.value == "Input",
                    "Expected `Input` direction, found `{}`",
                    attr.value
                ),
                "Wire" => wire_id = Some(attr.value),
                "DataType" => ensure!(
                    attr.value
                        == "NationalInstruments:SourceModel:DataTypes:X3SequenceWireDataType",
                    "Expected `Input` direction, found `{}`",
                    attr.value
                ),
                "Hotspot" => {}
                "Bounds" => {
                    bounds = Some(
                        parse_bounds(attr.value)
                            .context("Failed parsing bounds for sequence block")?,
                    )
                }
                _ => bail!("Unexpected sequence attribute: {name}"),
            }
        }
        let bounds = bounds.context("Failed finding bounds")?;
        let sequence_in = SequenceBlock {
            ty: SequenceBlockType::In,
            wire_id,
            bounds,
        };

        let Event::Empty(t) = self.next_event()? else {
            bail!("Expected empty tag for parsing sequence block");
        };

        let qname = t.name();
        let (name, prefix) =
            extract_name_from_qname(qname).context("Failed parsing empty tag name")?;
        let attributes = parse_attributes(&t).context("Failed parsing empty tag attributes")?;
        if let Some(prefix) = prefix {
            bail!("Unexpected prefix namespace {prefix} in `ConfigurableMethodCall` sequence tag");
        }
        ensure!(
            name == "Terminal",
            "Expected tag name Terminal, found `{name}`"
        );
        let mut wire_id = None;
        let mut bounds = None;
        for attr in attributes {
            let name = attr.key.0;
            match name.as_str() {
                "Id" => ensure!(
                    attr.value == "SequenceOut",
                    "Expected `SequenceOut` id, found `{}`",
                    attr.value
                ),
                "Direction" => ensure!(
                    attr.value == "Output",
                    "Expected `Output` direction, found `{}`",
                    attr.value
                ),
                "Wire" => wire_id = Some(attr.value),
                "DataType" => ensure!(
                    attr.value
                        == "NationalInstruments:SourceModel:DataTypes:X3SequenceWireDataType",
                    "Expected `Input` direction, found `{}`",
                    attr.value
                ),
                "Hotspot" => {}
                "Bounds" => {
                    bounds = Some(
                        parse_bounds(attr.value)
                            .context("Failed parsing bounds for sequence block")?,
                    )
                }
                _ => bail!("Unexpected sequence attribute: {name}"),
            }
        }
        let bounds = bounds.context("Failed finding bounds")?;
        let sequence_out = SequenceBlock {
            ty: SequenceBlockType::Out,
            wire_id,
            bounds,
        };
        Ok((sequence_in, sequence_out))
    }

    fn parse_wire_tag(
        &mut self,
        attributes: Vec<ParsedAttribute>,
    ) -> anyhow::Result<(String, Wire)> {
        let mut id = None;
        let mut seq_out = None;
        let mut seq_in = None;
        for attr in attributes {
            let name = attr.key.0.as_str();
            match name {
                "Id" => id = Some(attr.value),
                "Joints" => {
                    let s = self
                        .parse_joints(attr.value)
                        .context("Failed parsing joints")?;
                    seq_in = Some(s.0);
                    seq_out = Some(s.1);
                }
                _ => bail!("Unexpected attribute {name} in wire"),
            }
        }
        let seq_in = seq_in.context("Failed finding input")?;
        let seq_out = seq_out.context("Failed finding output")?;
        let id = id.context("Failed finding id")?;
        let wire = Wire {
            input: seq_in,
            output: seq_out,
        };
        Ok((id, wire))
    }

    fn parse_joints(&mut self, val: String) -> anyhow::Result<(String, String)> {
        let iter = val
            .split(' ')
            // "N(n1:sequenceout)" => ("N", "(n1:sequenceout)")
            .map(|s| (s.get(0..1).unwrap(), s.get(1..).unwrap()))
            // I assume the ones holding "N" are the ones which have sequences,
            // and the others, like h or w, have the other joints
            .filter(|(c, _)| *c == "N")
            // "(n1:sequenceout)" => ("n1", "sequenceout")
            // TODO: Propagate error instead of panicking
            .map(|(_, s)| {
                let idx = s.find(':').unwrap();
                let idx_paren = s.find(')').unwrap();
                (s.get(1..idx).unwrap(), s.get((idx + 1)..idx_paren).unwrap())
            });
        let mut seq_in = None;
        let mut seq_out = None;
        for (id, val) in iter {
            match val {
                "SequenceOut" => seq_out = Some(id),
                "SequenceIn" => seq_in = Some(id),
                _ => bail!("Unexpected value for joint: {val}"),
            }
        }
        let seq_in = seq_in.context("Expected input joint")?.to_owned();
        let seq_out = seq_out.context("Expected output joint")?.to_owned();
        Ok((seq_in, seq_out))
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

    pub fn build(self) -> anyhow::Result<File> {
        let name = self.name.context("No name found")?;
        let version = self.version.context("No version found")?;
        let decl = self.decl.context("No decl found")?;
        Ok(File {
            name,
            version,
            decl,
            blocks: self.blocks,
            wires: self.wires,
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
