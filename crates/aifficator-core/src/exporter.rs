use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;
use url::Url;

#[derive(Clone, Debug)]
pub struct ExportTrackReplacement {
    pub location: String,
    pub kind: String,
    pub size: Option<u64>,
    pub sample_rate: Option<u32>,
    pub bit_rate: Option<u32>,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("failed to parse XML while exporting: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("failed to decode XML attribute while exporting: {0}")]
    Attribute(String),
    #[error("failed to encode exported XML as UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("failed to write exported XML: {0}")]
    Io(#[from] std::io::Error),
    #[error("path cannot be represented as a file URL: {0}")]
    InvalidPath(String),
    #[error("PLAYLISTS section was not found in Rekordbox XML")]
    MissingPlaylistsSection,
}

pub fn export_replacement_xml(
    xml: &str,
    replacements: &BTreeMap<String, ExportTrackReplacement>,
) -> Result<String, ExportError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut in_collection = false;

    loop {
        match reader.read_event()? {
            Event::Start(event) => {
                let name = element_name(event.name().as_ref());

                if name == "COLLECTION" {
                    in_collection = true;
                    writer.write_event(Event::Start(event))?;
                } else if in_collection && name == "TRACK" {
                    writer.write_event(Event::Start(rewrite_track_start(
                        &reader,
                        &event,
                        replacements,
                    )?))?;
                } else {
                    writer.write_event(Event::Start(event))?;
                }
            }
            Event::Empty(event) => {
                let name = element_name(event.name().as_ref());

                if in_collection && name == "TRACK" {
                    writer.write_event(Event::Empty(rewrite_track_start(
                        &reader,
                        &event,
                        replacements,
                    )?))?;
                } else {
                    writer.write_event(Event::Empty(event))?;
                }
            }
            Event::End(event) => {
                let name = element_name(event.name().as_ref());
                writer.write_event(Event::End(event))?;

                if name == "COLLECTION" {
                    in_collection = false;
                }
            }
            Event::Eof => break,
            event => {
                writer.write_event(event)?;
            }
        }
    }

    Ok(String::from_utf8(writer.into_inner())?)
}

pub fn export_with_new_playlist_xml(
    xml: &str,
    playlist_name: &str,
    track_ids: &[String],
) -> Result<String, ExportError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut writer = Writer::new(Vec::with_capacity(xml.len() + track_ids.len() * 32));
    let mut injected = false;

    loop {
        match reader.read_event()? {
            Event::Empty(event) => {
                let name = element_name(event.name().as_ref());

                if name == "PLAYLISTS" {
                    writer.write_event(Event::Start(event.to_owned()))?;
                    write_generated_playlist(&mut writer, playlist_name, track_ids)?;
                    writer.write_event(Event::End(BytesEnd::new("PLAYLISTS")))?;
                    injected = true;
                } else {
                    writer.write_event(Event::Empty(event))?;
                }
            }
            Event::End(event) => {
                let name = element_name(event.name().as_ref());

                if name == "PLAYLISTS" && !injected {
                    write_generated_playlist(&mut writer, playlist_name, track_ids)?;
                    injected = true;
                }

                writer.write_event(Event::End(event))?;
            }
            Event::Eof => break,
            event => {
                writer.write_event(event)?;
            }
        }
    }

    if !injected {
        return Err(ExportError::MissingPlaylistsSection);
    }

    Ok(String::from_utf8(writer.into_inner())?)
}

pub fn path_to_rekordbox_location(path: &Path) -> Result<String, ExportError> {
    let url = Url::from_file_path(path)
        .map_err(|_| ExportError::InvalidPath(path.display().to_string()))?
        .to_string();

    Ok(url.replacen("file://", "file://localhost", 1))
}

fn rewrite_track_start(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    replacements: &BTreeMap<String, ExportTrackReplacement>,
) -> Result<BytesStart<'static>, ExportError> {
    let attrs = decoded_attributes(reader, event)?;
    let Some(track_id) = attrs.get("TrackID") else {
        return Ok(event.to_owned());
    };
    let Some(replacement) = replacements.get(track_id) else {
        return Ok(event.to_owned());
    };

    let mut rewritten = BytesStart::new(element_name(event.name().as_ref()));
    let mut saw_location = false;
    let mut saw_kind = false;
    let mut saw_size = false;
    let mut saw_sample_rate = false;
    let mut saw_bit_rate = false;

    for (key, value) in attrs {
        let rewritten_value = match key.as_str() {
            "Location" => {
                saw_location = true;
                replacement.location.clone()
            }
            "Kind" => {
                saw_kind = true;
                replacement.kind.clone()
            }
            "Size" => {
                saw_size = true;
                replacement
                    .size
                    .map(|size| size.to_string())
                    .unwrap_or(value)
            }
            "SampleRate" => {
                saw_sample_rate = true;
                replacement
                    .sample_rate
                    .map(|sample_rate| sample_rate.to_string())
                    .unwrap_or(value)
            }
            "BitRate" => {
                saw_bit_rate = true;
                replacement
                    .bit_rate
                    .map(|bit_rate| bit_rate.to_string())
                    .unwrap_or(value)
            }
            _ => value,
        };

        rewritten.push_attribute((key.as_str(), rewritten_value.as_str()));
    }

    if !saw_location {
        rewritten.push_attribute(("Location", replacement.location.as_str()));
    }
    if !saw_kind {
        rewritten.push_attribute(("Kind", replacement.kind.as_str()));
    }
    if !saw_size {
        if let Some(size) = replacement.size {
            rewritten.push_attribute(("Size", size.to_string().as_str()));
        }
    }
    if !saw_sample_rate {
        if let Some(sample_rate) = replacement.sample_rate {
            rewritten.push_attribute(("SampleRate", sample_rate.to_string().as_str()));
        }
    }
    if !saw_bit_rate {
        if let Some(bit_rate) = replacement.bit_rate {
            rewritten.push_attribute(("BitRate", bit_rate.to_string().as_str()));
        }
    }

    Ok(rewritten)
}

fn write_generated_playlist(
    writer: &mut Writer<Vec<u8>>,
    playlist_name: &str,
    track_ids: &[String],
) -> Result<(), ExportError> {
    let folder_count = "1".to_string();
    let mut folder = BytesStart::new("NODE");
    folder.push_attribute(("Name", "Rau Studio"));
    folder.push_attribute(("Type", "0"));
    folder.push_attribute(("Count", folder_count.as_str()));
    writer.write_event(Event::Start(folder))?;

    let entries = track_ids.len().to_string();
    let mut playlist = BytesStart::new("NODE");
    playlist.push_attribute(("Name", playlist_name));
    playlist.push_attribute(("Type", "1"));
    playlist.push_attribute(("KeyType", "0"));
    playlist.push_attribute(("Entries", entries.as_str()));
    writer.write_event(Event::Start(playlist))?;

    for track_id in track_ids {
        let mut track = BytesStart::new("TRACK");
        track.push_attribute(("Key", track_id.as_str()));
        writer.write_event(Event::Empty(track))?;
    }

    writer.write_event(Event::End(BytesEnd::new("NODE")))?;
    writer.write_event(Event::End(BytesEnd::new("NODE")))?;
    Ok(())
}

fn decoded_attributes(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
) -> Result<BTreeMap<String, String>, ExportError> {
    let mut attrs = BTreeMap::new();

    for attr in event.attributes() {
        let attr = attr.map_err(|error| ExportError::Attribute(error.to_string()))?;
        let key = element_name(attr.key.as_ref());
        let value = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(|error| ExportError::Attribute(error.to_string()))?;
        attrs.insert(key, value.into_owned());
    }

    Ok(attrs)
}

fn element_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}
