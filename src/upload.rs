//! `--file`: sends one file through an Upload link and yields its upload id.
//!
//! The link takes the bytes without a bearer: a file up to `partSize` goes as one `PUT <url>`, a larger one as
//! `PUT <url>?offset=N` parts and then `POST <url>/finish`. A 409 naming `bytesReceived` says where to go on
//! from; after a request that got no usable answer, `GET <url>` says it. Once a whole-file `PUT` has failed, the
//! rest goes as parts, because the link refuses a second whole file once it holds any byte.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use ureq::Agent;
use ureq::http::{Method, Response, header};

use crate::failure::{ErrorCode, Failure};
use crate::generated::UploadSlot;
use crate::http::{self, Client};
use crate::response;

const UPLOADS_PATH: &str = "/api/v1/uploads";
/// Tries in a row that move no byte forward before the upload gives up.
const MAX_STALLED_TRIES: u32 = 5;

/// Uploads every file in `paths` through its own Upload link for `slot` and returns the upload ids: a list when
/// the slot takes one, else the one id. Every file is opened before the first request.
pub fn send_all(
    client: &mut Client,
    slot: &UploadSlot,
    paths: &[&PathBuf],
) -> Result<Value, Failure> {
    let sources = paths
        .iter()
        .map(|path| Source::open(path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut ids = Vec::new();
    for source in sources {
        ids.push(send(client, slot.target, source)?);
    }
    Ok(if slot.parameter.schema.array {
        Value::Array(ids)
    } else {
        ids.swap_remove(0)
    })
}

/// One file to send, opened.
struct Source<'a> {
    file: File,
    path: &'a Path,
    file_name: String,
    length: u64,
}

impl<'a> Source<'a> {
    fn open(path: &'a Path) -> Result<Self, Failure> {
        let file = File::open(path).map_err(|error| Failure::file(path, error))?;
        let length = file
            .metadata()
            .map_err(|error| Failure::file(path, error))?
            .len();
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| {
                Failure::local(
                    ErrorCode::File,
                    format!("{} names no file.", path.display()),
                )
            })?;
        Ok(Source {
            file,
            path,
            file_name,
            length,
        })
    }
}

/// Uploads `source` to a new Upload link for `target` and returns the link's upload id.
fn send(client: &mut Client, target: &str, source: Source) -> Result<Value, Failure> {
    let request = json!({ "target": target, "fileName": source.file_name });
    let answer = client.send(
        &Method::POST,
        UPLOADS_PATH,
        Some(request.to_string().as_bytes()),
    )?;
    let link = Link::read(&response::read_json(answer)?, client.host())?;
    if let Some(max_bytes) = link
        .max_bytes
        .filter(|max_bytes| source.length > *max_bytes)
    {
        return Err(Failure::local(
            ErrorCode::File,
            format!(
                "{} is {} bytes; the upload target `{target}` takes at most {max_bytes}.",
                source.path.display(),
                source.length
            ),
        ));
    }

    Sender {
        agent: http::agent(),
        link: &link,
        source,
    }
    .send()?;
    Ok(link.upload_id)
}

/// The answer of `POST /api/v1/uploads`.
struct Link {
    url: String,
    upload_id: Value,
    part_size: u64,
    max_bytes: Option<u64>,
}

impl Link {
    fn read(answer: &Value, host: &str) -> Result<Self, Failure> {
        let invalid = |field: &str| {
            Failure::invalid_response(format!("The Upload link has no valid `{field}`."))
        };
        let url = answer["url"].as_str().ok_or_else(|| invalid("url"))?;
        let url = if url.starts_with('/') {
            format!("{host}{url}")
        } else {
            url.to_string()
        };
        let upload_id = answer
            .get("uploadId")
            .filter(|id| id.is_string())
            .ok_or_else(|| invalid("uploadId"))?
            .clone();
        let part_size = answer["partSize"]
            .as_u64()
            .filter(|size| *size > 0)
            .ok_or_else(|| invalid("partSize"))?;
        let max_bytes = match &answer["maxBytes"] {
            Value::Null => None,
            max_bytes => Some(max_bytes.as_u64().ok_or_else(|| invalid("maxBytes"))?),
        };
        Ok(Link {
            url,
            upload_id,
            part_size,
            max_bytes,
        })
    }
}

/// How the link took one request, as `classify` reads its answer.
enum Answer {
    /// A 2xx, with the upload's status when the body carried one.
    Accepted(Option<Status>),
    /// A 409 naming the bytes the link holds: go on from there.
    Holds(u64),
    /// A network error or a 5xx: ask the link where it stands.
    Lost,
}

struct Status {
    bytes_received: u64,
    finished: bool,
}

impl Status {
    fn read(value: &Value) -> Option<Self> {
        Some(Status {
            bytes_received: value["bytesReceived"].as_u64()?,
            finished: value["finished"] == true,
        })
    }
}

struct Sender<'a> {
    agent: Agent,
    link: &'a Link,
    source: Source<'a>,
}

impl Sender<'_> {
    fn send(&mut self) -> Result<(), Failure> {
        let length = self.source.length;
        // `None` sends the whole file in one `PUT`. `Some(offset)` sends the part at `offset`, or finishes once
        // `offset` is the file's length. Every request that does not end the upload leaves `Some`, so the rest goes
        // as parts.
        let mut offset = (length > self.link.part_size).then_some(0);
        let mut stalled = 0;
        loop {
            let before = offset.unwrap_or(0);
            let answer = match offset {
                Some(offset) if offset == length => self.finish()?,
                offset => self.put(offset)?,
            };
            let received = match answer {
                Answer::Accepted(_) if offset.is_none_or(|offset| offset == length) => {
                    return Ok(());
                }
                Answer::Accepted(status) => status.map_or_else(
                    || before + self.part_length(before),
                    |status| status.bytes_received,
                ),
                Answer::Holds(received) => received,
                Answer::Lost => match self.status()? {
                    Some(status) if status.finished => return Ok(()),
                    Some(status) => status.bytes_received,
                    None => before,
                },
            };
            if received > length {
                return Err(self.failure(format!(
                    "the Upload link holds {received} bytes, more than the file's {length}"
                )));
            }
            offset = Some(received);
            if received > before {
                stalled = 0;
            } else {
                stalled += 1;
                if stalled == MAX_STALLED_TRIES {
                    return Err(self.failure(format!(
                        "{MAX_STALLED_TRIES} tries in a row moved no byte forward; the Upload link holds {received} of {length} bytes"
                    )));
                }
            }
        }
    }

    fn part_length(&self, offset: u64) -> u64 {
        self.link.part_size.min(self.source.length - offset)
    }

    /// `PUT <url>` with the whole file, or `PUT <url>?offset=N` with the part at `offset`.
    fn put(&mut self, offset: Option<u64>) -> Result<Answer, Failure> {
        let start = offset.unwrap_or(0);
        let bytes = self.read(start, self.part_length(start))?;
        let url = match offset {
            Some(offset) => format!("{}?offset={offset}", self.link.url),
            None => self.link.url.clone(),
        };
        let result = self
            .agent
            .put(&url)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .send(&bytes[..]);
        classify(&url, result)
    }

    fn finish(&self) -> Result<Answer, Failure> {
        let url = format!("{}/finish", self.link.url);
        let body = json!({ "totalBytes": self.source.length, "fileName": self.source.file_name });
        let result = self
            .agent
            .post(&url)
            .header(header::CONTENT_TYPE, "application/json")
            .send(body.to_string());
        classify(&url, result)
    }

    /// `GET <url>`: where the upload stands, or `None` when the link could not say.
    fn status(&self) -> Result<Option<Status>, Failure> {
        let result = self
            .agent
            .get(&self.link.url)
            .header(header::ACCEPT, "application/json")
            .call();
        Ok(match classify(&self.link.url, result)? {
            Answer::Accepted(status) => status,
            Answer::Holds(_) | Answer::Lost => None,
        })
    }

    /// The `length` bytes at `offset`, read from the file on their own.
    fn read(&mut self, offset: u64, length: u64) -> Result<Vec<u8>, Failure> {
        let path = self.source.path;
        let file_failure = |error: io::Error| Failure::file(path, error);
        self.source
            .file
            .seek(SeekFrom::Start(offset))
            .map_err(file_failure)?;
        let mut bytes = Vec::new();
        (&mut self.source.file)
            .take(length)
            .read_to_end(&mut bytes)
            .map_err(file_failure)?;
        if (bytes.len() as u64) < length {
            return Err(Failure::local(
                ErrorCode::File,
                format!("{} changed while it was being sent.", path.display()),
            ));
        }
        Ok(bytes)
    }

    fn failure(&self, reason: String) -> Failure {
        Failure::local(
            ErrorCode::Upload,
            format!("{}: {reason}.", self.source.path.display()),
        )
    }
}

fn classify(
    url: &str,
    result: Result<Response<ureq::Body>, ureq::Error>,
) -> Result<Answer, Failure> {
    let response = match result {
        Ok(response) => response,
        Err(error @ (ureq::Error::BadUri(_) | ureq::Error::Http(_))) => {
            return Err(http::transport_failure(url, error));
        }
        Err(_) => return Ok(Answer::Lost),
    };
    let status = response.status().as_u16();
    let content_type = response::header_text(&response, header::CONTENT_TYPE);
    let mut bytes = Vec::new();
    if response
        .into_body()
        .into_reader()
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Ok(Answer::Lost);
    }
    let value = serde_json::from_slice::<Value>(&bytes).ok();
    match status {
        200..=299 => Ok(Answer::Accepted(value.as_ref().and_then(Status::read))),
        409 => match value
            .as_ref()
            .and_then(|value| value["bytesReceived"].as_u64())
        {
            Some(received) => Ok(Answer::Holds(received)),
            None => Err(response::problem(status, content_type.as_deref(), bytes)),
        },
        500.. => Ok(Answer::Lost),
        _ => Err(response::problem(status, content_type.as_deref(), bytes)),
    }
}
