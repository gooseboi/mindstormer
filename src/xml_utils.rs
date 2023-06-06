use crate::utils::VecReadWrapper;
use anyhow::Context;
use quick_xml::{
    events::{
        attributes::{Attribute, Attributes},
        BytesStart, Event,
    },
    name::QName,
    reader::Reader,
};
pub type XMLReader = Reader<VecReadWrapper>;

pub fn collect_to_vec(mut reader: XMLReader) -> anyhow::Result<Vec<Event<'static>>> {
    let mut result = vec![];
    loop {
        let mut buf = vec![];
        let event = reader
            .read_event_into(&mut buf)
            .context("File had invalid xml")?
            .into_owned();
        if let Event::Eof = event {
            break Ok(result);
        } else {
            result.push(event);
        }
    }
}

pub fn extract_name_from_qname(qname: QName) -> anyhow::Result<(String, Option<String>)> {
    let (name, prefix) = qname.decompose();
    let name = String::from_utf8(name.into_inner().to_vec()).context("Invalid UTF-8 in tag")?;
    let prefix = prefix
        .map(|s| String::from_utf8(s.into_inner().to_vec()))
        .transpose()
        .context("Invalid UTF-8 in tag prefix")?;
    Ok((name, prefix))
}

#[derive(Debug)]
pub struct ParsedAttribute {
    pub key: (String, Option<String>),
    pub value: String,
}

impl ParsedAttribute {
    fn parse(attr: &Attribute) -> anyhow::Result<ParsedAttribute> {
        let (name, prefix) = extract_name_from_qname(attr.key)?;
        let value = String::from_utf8(attr.value.clone().into_owned())
            .context(format!("Invalid UTF-8 in {name} tag value"))?;
        Ok(ParsedAttribute {
            key: (name, prefix),
            value,
        })
    }
}

pub trait HasAttribute {
    fn attributes(&self) -> Attributes;
}

impl HasAttribute for BytesStart<'_> {
    fn attributes(&self) -> Attributes {
        self.attributes()
    }
}

pub fn parse_attributes(tag: &impl HasAttribute) -> anyhow::Result<Vec<ParsedAttribute>> {
    let mut v = vec![];
    for attr in tag.attributes() {
        let attr = attr.context("Malformed attribute")?;
        v.push(ParsedAttribute::parse(&attr).context("Failed parsing attribute")?);
    }
    Ok(v)
}
