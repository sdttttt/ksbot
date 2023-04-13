use std::str::FromStr;

use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};

use super::buf::BufPool;
use super::item::FeedPost;
use super::utils::attrs_get_str;
use super::utils::{parse_atom_link, AtomLink, NumberData, TextOrCData};
use super::{FromXmlWithBufRead, FromXmlWithReader, FromXmlWithStr, SkipThisElement};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "rss")]
pub struct Feed {
    #[serde(skip)]
    pub version: Option<String>,

    pub title: String,

    pub description: Option<String>,

    pub link: String,

    #[serde(rename = "atomLink")]
    pub atom_link: Option<String>,

    pub language: Option<String>,

    #[serde(rename = "webMaster")]
    pub web_master: Option<String>,

    pub generator: Option<String>,

    #[serde(rename = "lastBuildDate")]
    pub last_build_date: Option<String>,

    pub ttl: Option<u32>,

    pub image: Option<ChannelImage>,

    pub posts: Vec<FeedPost>,

    pub copyright: Option<String>,
}

impl FromXmlWithStr for Feed {
    /// > It takes a string, creates a reader from it, and then calls the function that takes a reader
    ///
    /// Arguments:
    ///
    /// * `bufs`: &BufPool - this is a pool of buffers that the parser uses to store the data it reads.
    /// * `text`: The XML text to parse.
    ///
    /// Returns:
    ///
    /// A `fast_xml::Result<RSSChannel>`
    fn from_xml_with_str(bufs: &BufPool, text: &str) -> quick_xml::Result<Feed> {
        let mut reader = Reader::from_str(text);

        Self::from_xml_with_reader(bufs, &mut reader)
    }
}

impl FromXmlWithBufRead for Feed {
    fn from_xml_with_buf<B: std::io::BufRead>(buf_read: B) -> quick_xml::Result<Self> {
        let bufs: BufPool = Default::default();
        let mut reader = Reader::from_reader(buf_read);
        Self::from_xml_with_reader(&bufs, &mut reader)
    }
}

impl FromXmlWithReader for Feed {
    /// > Read the XML document, and for each element, if it's a `<rss>` element, get the `version`
    /// attribute, if it's a `<channel>` element, read the `<title>`, `<description>`, `<link>` and
    /// `<item>` elements, and if it's a `<item>` element, read the `<title>`, `<description>`, `<link>`,
    /// `<pubDate>`, `<guid>` and `<category>` elements
    ///
    /// Arguments:
    ///
    /// * `bufs`: &BufPool - this is a pool of buffers that are used to read the XML.
    /// * `reader`: &mut Reader<B>
    ///
    /// Returns:
    ///
    /// A `Result` of `Channel`
    fn from_xml_with_reader<B: std::io::BufRead>(
        bufs: &BufPool,
        reader: &mut Reader<B>,
    ) -> quick_xml::Result<Self> {
        let mut version = None;
        let mut title = "".to_owned();
        let mut description = None;
        let mut url = "".to_owned();
        let mut atom_link = None;
        let mut language = None;
        let mut web_master = None;
        let mut last_build_date = None;
        let mut ttl = None;
        let mut image = None;
        let mut posts = Vec::<FeedPost>::new();
        let mut copyright = None;
        let mut generator = None;

        reader.trim_text(true);

        let mut buf = bufs.pop();

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Empty(ref ce)) => match reader.decode(ce.local_name())? {
                    "link" => match parse_atom_link(reader, ce.attributes())? {
                        Some(AtomLink::Alternate(link)) => url = link,
                        Some(AtomLink::Source(link)) => atom_link = Some(link),
                        _ => {}
                    },

                    _ => (),
                },

                Ok(Event::Start(ref re)) => match reader.decode(re.local_name())? {
                    "rss" => version = attrs_get_str(&reader, re.attributes(), "version")?,

                    "channel" => {}

                    "title" => {
                        title = TextOrCData::from_xml_with_reader(bufs, reader)?
                            .expect("没有title的订阅源？！");
                    }

                    "description" => {
                        description = TextOrCData::from_xml_with_reader(bufs, reader)?;
                    }

                    "link" => {
                        match TextOrCData::from_xml_with_reader(bufs, reader)? {
                            Some(s) => url = s,
                            None => match parse_atom_link(reader, re.attributes())? {
                                Some(AtomLink::Source(e)) => atom_link = Some(e),
                                Some(AtomLink::Alternate(e)) => url = e,
                                _ => (),
                            },
                        };
                    }

                    "language" => language = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "webMaster" => web_master = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "generator" => generator = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    "lastBuildDate" => {
                        last_build_date = TextOrCData::from_xml_with_reader(bufs, reader)?
                    }

                    "ttl" => ttl = NumberData::from_xml_with_reader(bufs, reader)?,

                    "image" => image = Some(ChannelImage::from_xml_with_reader(bufs, reader)?),

                    "item" | "entry" => {
                        let item = FeedPost::from_xml_with_reader(bufs, reader)?;
                        posts.push(item);
                    }

                    "copyright" => copyright = TextOrCData::from_xml_with_reader(bufs, reader)?,

                    _ => {
                        SkipThisElement::from_xml_with_reader(bufs, reader)?;
                    }
                },
                Ok(Event::Eof | Event::End(_)) => break,
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),

                _ => (),
            }
            buf.clear();
        }

        Ok(Self {
            version,
            title,
            description,
            link: url,
            atom_link,
            language,
            web_master,
            generator,
            last_build_date,
            ttl,
            image,
            posts,
            copyright,
        })
    }
}

impl FromStr for Feed {
    type Err = quick_xml::Error;

    /// It takes a string, parses it as XML, and returns a `fast_xml::Result<RSSChannel>`
    ///
    /// Arguments:
    ///
    /// * `text`: The XML text to parse.
    ///
    /// Returns:
    ///
    /// A Result<RSSChannel, fast_xml::Error>
    fn from_str(text: &str) -> quick_xml::Result<Feed> {
        let bufs = BufPool::new(16, 2048);

        Self::from_xml_with_str(&bufs, text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "image")]
pub struct ChannelImage {
    url: Option<String>,
}

impl FromXmlWithReader for ChannelImage {
    fn from_xml_with_reader<B: std::io::BufRead>(
        bufs: &BufPool,
        reader: &mut Reader<B>,
    ) -> quick_xml::Result<Self> {
        let mut url = None;

        let mut buf = bufs.pop();

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => match reader.decode(e.local_name())? {
                    "url" => url = TextOrCData::from_xml_with_reader(bufs, reader)?,
                    _ => {
                        SkipThisElement::from_xml_with_reader(bufs, reader)?;
                    }
                },
                Ok(Event::End(_) | Event::Eof) => break,
                Ok(_) => (),

                Err(e) => return Err(e),
            }
            buf.clear();
        }

        Ok(Self { url })
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn encoding() {
        let s: &[u8] = include_bytes!("../../test/data/rss_2.0.xml");
        let r = Feed::from_xml_with_buf(Cursor::new(s)).unwrap();
        assert_eq!(r.title, "rss_2.0.channel.title".to_owned());
        assert_eq!(r.link, "rss_2.0.channel.link".to_owned());
        assert_eq!(
            r.description,
            Some("rss_2.0.channel.description".to_owned())
        );
    }

    #[test]
    fn samdm_test() {
        let s: &[u8] = include_bytes!("../../test/data/3dm.xml");
        let r = Feed::from_xml_with_buf(Cursor::new(s)).unwrap();
        assert_eq!(r.title, "3DM - 新闻中心".to_owned());
        assert_eq!(r.link, "http://www.3dmgame.com/news/".to_owned());
        assert_eq!(
            r.description,
            Some(
                "3DM - 新闻中心 - Made with love by RSSHub(https://github.com/DIYgod/RSSHub)"
                    .to_owned()
            )
        );
    }
}
