use std::str::FromStr;

use quick_xml::Result;
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};

use super::buf::BufPool;
use super::utils::TextOrCData;
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
        let mut title = None;
        let mut description = None;
        let mut pub_date = None;
        let mut guid = None;
        let mut link = None;
        let mut author = None;
        let mut category = Vec::<String>::new();

        reader.trim_text(true);
        let mut buf = bufs.pop();

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => match reader.decode(e.name())? {
                    "title" => title = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "pubDate" => pub_date = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "guid" => guid = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "link" => link = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "description" => description = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "author" => author = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "category" => {
                        let category_item = TextOrCData::from_xml_with_reader(bufs, reader)?;
                        if let Some(c) = category_item {
                            category.push(c);
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

        Ok(Self {
            title,
            description,
            pub_date,
            guid,
            link,
            author,
            category,
        })
    }
}

impl FromStr for FeedPost {
    type Err = quick_xml::Error;

    fn from_str(text: &str) -> Result<FeedPost> {
        let bufs = BufPool::new(64, 8192);

        Self::from_xml_with_str(&bufs, text)
    }
}
