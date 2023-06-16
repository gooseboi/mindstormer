use super::parser::{Block, FileBuilder, Id, Wire};
use crate::utils::VecReadWrapper;
use anyhow::{bail, Context};
use quick_xml::{events::BytesDecl, reader::Reader};
use std::collections::HashMap;
use std::fs;
use std::io::Read;

#[derive(Clone, Default, Debug)]
pub struct Version {
    pub number: String,
    pub namespace: String,
}

pub struct File {
    pub decl: BytesDecl<'static>,
    pub version: Version,
    pub name: String,
    pub blocks: HashMap<Id, Block>,
    pub wires: HashMap<Id, Wire>,
}

impl File {
    fn new(name: &str, contents: Vec<u8>) -> anyhow::Result<Self> {
        let wrapper = VecReadWrapper::new(contents);
        let mut xml = Reader::from_reader(wrapper);
        xml.trim_text(true);
        let mut builder = FileBuilder::from_xml(xml)?;
        builder.name(name.into())?;
        builder.parse().context("Failed parsing file contents")?;
        builder.build().context("Failed building file struct")
    }
}

pub struct Project {
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
    files: Vec<File>,
}

impl Project {
    pub fn output_file(&self, fname: &str) -> anyhow::Result<()> {
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
            let _ = &f.blocks;
        }
        bail!("Outputting the project not yet implemented")
    }
    pub fn get_project_from_zip(filename: &str) -> anyhow::Result<Self> {
        let file = fs::File::open(filename)?;
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
                    description =
                        Some(String::from_utf8(bytes).context("Invalid description data")?)
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
                    files.push(File::new(name, bytes).context(format!("Failed parsing {name}"))?);
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

        Ok(Self {
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
}
