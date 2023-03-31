mod buf;
mod feed;
mod http;
mod item;
mod utils;

use std::io::BufRead;

use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader as XmlReader;

use self::buf::BufPool;
use self::feed::RSSChannel;

pub const RSS_VERSION_AVAILABLE: &str = "2.0";

pub trait FromXmlWithStr: Sized {
    fn from_xml_with_str(bufs: &BufPool, text: &str) -> quick_xml::Result<Self>;
}

pub trait FromXmlWithBufRead: Sized {
    fn from_xml_with_buf<B: std::io::BufRead>(bufs: B) -> quick_xml::Result<Self>;
}

/// A trait that is implemented by the structs that are used to parse the XML.
pub trait FromXmlWithReader: Sized {
    /// A function that takes a BufPool and a string and returns a fast_xml::Result<Self>
    fn from_xml_with_reader<B: BufRead>(
        bufs: &BufPool,
        reader: &mut XmlReader<B>,
    ) -> quick_xml::Result<Self>;
}

struct SkipThisElement;
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
    ) -> quick_xml::Result<Self> {
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

pub use http::*;
