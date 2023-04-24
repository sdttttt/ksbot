use std::str::FromStr;

use quick_xml::Result;
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};

use super::buf::BufPool;
use super::utils::{parse_atom_link, AtomLink, TextOrCData};
use super::FromXmlWithReader;
use super::FromXmlWithStr;
use super::SkipThisElement;

#[derive(Debug, Clone, Serialize, Default, Deserialize)]
#[serde(rename = "item")]
pub struct FeedPost {
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "pubDate")]
    pub pub_date: Option<String>,
    pub guid: Option<String>,
    pub link: Option<String>,
    pub author: Option<String>,
    pub category: Vec<String>,
}

impl FromXmlWithStr for FeedPost {
    /// `Self::from_xml_with_reader(bufs, &mut reader)`
    ///
    /// The `Self` is a reference to the current type, which is `ChannelItem`
    ///
    /// Arguments:
    ///
    /// * `bufs`: &BufPool - this is a pool of buffers that the parser uses to store the data it reads.
    /// * `text`: &str - the XML string to parse
    ///
    /// Returns:
    ///
    /// A ChannelItem
    fn from_xml_with_str(bufs: &BufPool, text: &str) -> quick_xml::Result<FeedPost> {
        let mut reader = Reader::from_str(text);

        Self::from_xml_with_reader(bufs, &mut reader)
    }
}

impl FromXmlWithReader for FeedPost {
    /// > The function reads the XML events from the reader, and when it encounters a start tag, it calls
    /// the `from_xml_with_reader` function of the corresponding field
    ///
    /// Arguments:
    ///
    /// * `bufs`: &BufPool
    /// * `reader`: &mut Reader<B>
    ///
    /// Returns:
    ///
    /// a Result<Self>
    fn from_xml_with_reader<B: std::io::BufRead>(
        bufs: &BufPool,
        reader: &mut Reader<B>,
    ) -> quick_xml::Result<Self> {
        let mut post = FeedPost::default();

        reader.trim_text(true);
        let mut buf = bufs.pop();

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Empty(ref ce)) => match reader.decode(ce.local_name())? {
                    "link" => {
                        if let Some(AtomLink::Alternate(l)) =
                            parse_atom_link(reader, ce.attributes())?
                        {
                            post.link = Some(l);
                        }
                    }

                    _ => (),
                },

                Ok(Event::Start(ref e)) => match reader.decode(e.name())? {
                    "title" => post.title = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "pubDate" => post.pub_date = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "guid" => post.guid = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "link" => post.link = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "description" => {
                        post.description = TextOrCData::from_xml_with_reader(bufs, reader)?
                    }

                    "author" => {
                        post.author = TextOrCData::from_xml_with_reader(bufs, reader)?;
                    }

                    "category" => {
                        let category_item = TextOrCData::from_xml_with_reader(bufs, reader)?;
                        if let Some(c) = category_item {
                            post.category.push(c);
                        }
                    }

                    _ => {
                        SkipThisElement::from_xml_with_reader(bufs, reader)?;
                    }
                },

                Ok(Event::Eof | Event::End(_)) => break,

                Ok(_) => (),

                Err(e) => return Err(e),
            }
            buf.clear();
        }

        Ok(post)
    }
}

impl FromStr for FeedPost {
    type Err = quick_xml::Error;

    fn from_str(text: &str) -> Result<FeedPost> {
        let bufs = BufPool::new(64, 8192);

        Self::from_xml_with_str(&bufs, text)
    }
}
