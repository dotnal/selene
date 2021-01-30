use anyhow::Context;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Clone)]
pub struct SubPlaylist {
    pub url: String,
    pub attribs: HashMap<String, String>,
}

#[derive(Clone)]
pub struct TrackInfo {
    info: String,
    pub name: String,
}

#[derive(Clone)]
pub struct KeyInfo {
    pub method: String,
    pub uri: String, 
    pub iv: String,
}

#[derive(Clone)]
pub struct M3U {
    pub directives: HashMap<String, String>,
    pub tracklist: Vec<TrackInfo>,
    pub subplaylists: Vec<SubPlaylist>,
    pub key_info: Option<KeyInfo>,
}

impl M3U {
    pub fn is_primary(&self) -> bool {
        if self.subplaylists.len() > 0 && self.tracklist.len() == 0 {
            return true;
        }

        if self.subplaylists.len() == 0 && self.tracklist.len() > 0 {
            return false;
        }
        unreachable!("invalid m3u file provided")
    }
}

impl FromStr for M3U {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut csv_parser = csv::ReaderBuilder::new();
        csv_parser.has_headers(false);
        let mut lines = s.lines();
        if lines.next().context("empty file")? != "#EXTM3U" {
            anyhow::bail!("directive header not found");
        };

        let mut tracklist = vec![];
        let mut directives = HashMap::new();
        let mut sub_indices = vec![];
        let mut key_info = None;
        

        while let Some(line) = lines.next() {
            // secondary sources / subindices
            if line.starts_with("#EXT-X-STREAM-INF:") {
                let (_, value) = read_directive(line)?;
                let url = {
                    lines
                        .next()
                        .context("unexpected eof parsing sources listing")?
                };
                let mut rdr = csv_parser.from_reader(value.as_bytes());
                let record = rdr
                    .records()
                    .next()
                    .context("could not find valid CSV attrib for x-stream-inf")?
                    .context("could not unwrap inner StringRecord for ext-stream-inf")?;

                let mut attribs = HashMap::new();
                for r in record.iter() {
                    let split: Vec<_> = r.splitn(2, "=").collect();
                    if split.len() == 2 {
                        let (k, v) = (split[0], split[1]);
                        attribs.insert(k.into(), v.into());
                    }
                }

                sub_indices.push(SubPlaylist {
                    url: url.into(),
                    attribs,
                });
            }
            // key
            else if line.starts_with("#EXT-X-KEY:") {
                let (_, value) = read_directive(line)?;

                let mut rdr = csv_parser.from_reader(value.as_bytes());
                let record = rdr
                    .records()
                    .next()
                    .context("could not read CSV attrib for ext-x-key")?
                    .context("could not unwrap inner StringRecord for ext-x-key")?;

                let mut ki = KeyInfo{
                    method: "".into(),
                    uri: "".into(),
                    iv: "".into(),
                };

                for r in record.iter() {
                    let split: Vec<_> = r.splitn(2, "=").collect();
                    if split.len() == 2 {
                        let (k, v) = (split[0].to_lowercase(), split[1]);
                        match k.as_str() {
                            "iv" => ki.iv = v.into(),
                            "uri" => ki.uri = v.into(),
                            "method" => ki.method = v.into(),
                            _ => {}
                        }
                    }
                }

                key_info = Some(ki);
            }
            // track listing
            else if line.starts_with("#EXTINF:") {
                let info = read_directive(line)?.1.into();
                let name = {
                    lines
                        .next()
                        .context("unexpected eof parsing track listing")?
                }
                .into();
                tracklist.push(TrackInfo { info, name });
            }
            // end of the track listing
            else if line == "#EXT-X-ENDLIST" {
                break;
            }
            // other directive
            else if line.starts_with('#') {
                let (key, value) = read_directive(line)?;
                directives.insert(key.into(), value.into());
            }
            // unknown line
            else {
                println!("unknown line detected: [{}]", line);
            }
        }

        Ok(Self {
            directives,
            tracklist,
            subplaylists: sub_indices,
            key_info,
        })
    }
}

fn read_directive(line: &str) -> anyhow::Result<(&str, &str)> {
    let mut l = line.splitn(2, ":");
    Ok((
        l.next()
            .context(format!("directive key not found: [{}]", line))?,
        l.next()
            .context(format!("directive value not found: [{}]", line))?,
    ))
}
