mod buf;
mod feed;
mod item;
mod utils;

pub const RSS_VERSION_AVAILABLE: &str = "2.0";

pub fn parse_rss_channel(rss_text: &str) -> RSSChannel {
    RSSChannel::from_str(rss_text).unwrap()
}

pub trait FromXmlWithStr: Sized {
    fn from_xml_with_str(bufs: &BufPool, text: &str) -> fast_xml::Result<Self>;
}

/// A trait that is implemented by the structs that are used to parse the XML.
pub trait FromXmlWithReader: Sized {
    /// A function that takes a BufPool and a string and returns a fast_xml::Result<Self>
    fn from_xml_with_reader<B: BufRead>(
        bufs: &BufPool,
        reader: &mut XmlReader<B>,
    ) -> fast_xml::Result<Self>;
}

struct SkipThisElement;

use std::io::BufRead;
use std::str::FromStr;

use fast_xml::events::Event as XmlEvent;
use fast_xml::Reader as XmlReader;

use self::buf::BufPool;
use self::feed::RSSChannel;

impl FromXmlWithReader for SkipThisElement {
    /// "Skip the current element and all its children."
    ///
    /// The function is called `from_xml_with_reader` because it's part of a trait called
    /// `FromXmlWithReader` that's implemented for all types that can be deserialized from XML
    ///
    /// Arguments:
    ///
    /// * `bufs`: &BufPool
    /// * `reader`: &mut XmlReader<B>
    ///
    /// Returns:
    ///
    /// A SkipThisElement
    #[inline]
    fn from_xml_with_reader<B: BufRead>(
        bufs: &BufPool,
        reader: &mut XmlReader<B>,
    ) -> fast_xml::Result<Self> {
        let mut buf = bufs.pop();
        let mut depth = 1usize;
        loop {
            match reader.read_event(&mut buf) {
                Ok(XmlEvent::Start(_)) => depth += 1,
                Ok(XmlEvent::End(_)) if depth == 1 => break,
                Ok(XmlEvent::End(_)) => depth -= 1,
                Ok(XmlEvent::Eof) => break, // just ignore EOF
                Err(err) => return Err(err),
                _ => (),
            }
            buf.clear();
        }
        Ok(SkipThisElement)
    }
}
