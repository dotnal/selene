use crate::mp3url::M3U;
use anyhow::Context;
use anyhow::Result;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time::{delay_for, Duration};

// the www is important
pub const SCHOOLISM_URL: &'static str = "https://www.schoolism.com";
static LOGIN_FAILED_RE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"login\.colorBox\.php\?loginError=true").unwrap());

// chrome user agent
const USER_AGENT: &'static str = "Mozilla/5.0 (Windows NT 6.1; WOW64) \
    AppleWebKit/537.36 (KHTML, like Gecko) \
    Chrome/86.0.4230.1 \
    Safari/537.36";

// firefox user agent
// const USER_AGENT: &'static str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:82.0) \
//     Gecko/20100101 Firefox/82.0";

pub struct ClientInit {
    net_client: Arc<reqwest::Client>,
    username: String,
    password: String,
}

pub struct ClientConnected {
    net_client: Arc<reqwest::Client>,
}

impl ClientInit {
    pub fn new(username: &str, password: &str) -> anyhow::Result<Self> {
        let username = username.into();
        let password = password.into();
        let net_client = reqwest::ClientBuilder::new()
            .cookie_store(true)
            .user_agent(USER_AGENT)
            .build()
            .context("could not create new http client")?
            .into();

        Ok(Self {
            net_client,
            username,
            password,
        })
    }

    pub async fn connect(self) -> anyhow::Result<ClientConnected> {
        let net_client = self.net_client;
        let form = reqwest::multipart::Form::new()
            .text("email", self.username)
            .text("password", self.password)
            .text("submit", "Login");

        let resp = net_client
            .post(SCHOOLISM_URL)
            .multipart(form)
            .send()
            .await
            .context("could not submit login form")?;

        let page = resp.text().await.context("could not get page text")?;

        if LOGIN_FAILED_RE.is_match(&page) {
            anyhow::bail!("login failed")
        }

        Ok(ClientConnected { net_client })
    }
}

#[derive(Debug)]
pub struct Key(Vec<u8>);
impl ClientConnected {
    pub async fn get_playlist(
        &self,
        lesson_idx: usize,
        part_idx: usize,
        hq: bool,
    ) -> anyhow::Result<SchoolismVideoList> {
        let dashboard_page = self
            .net_client
            .get(&format!("{}/dashboard.php", SCHOOLISM_URL))
            .send()
            .await
            .context("failed to fetch dashboard page")?
            .text()
            .await
            .context("failed to access text content of dashboard page")?;

        let lessons = super::extractor::parse_dashboard(&dashboard_page)?;
        let lesson = &lessons
            .get(lesson_idx)
            .context(format!("lesson index [{}] does not exist", lesson_idx))?;

        let lesson_page = self
            .net_client
            .get(&format!("{}/{}", SCHOOLISM_URL, lesson.link))
            .send()
            .await
            .context("failed to fetch lesson page")?
            .text()
            .await
            .context("failed to access text content of lesson page")?;

        let playlists = super::extractor::parse_lesson(&lesson_page)
            .context("failed to parse lesson page into playlists")?;

        let playlist = playlists
            .get(part_idx)
            .context(format!("part index [{}] does not exist", part_idx))?;

        // wait a bit here to avoid timing issues
        delay_for(Duration::from_millis(1000)).await;

        // retry a few times, sometimes this call is flaky
        let mut retry_count: u32 = 0;
        let playlist = loop {
            let playlist = self
                .net_client
                .get(&playlist.url)
                .send()
                .await
                .context("failed to fetch playlist")?
                .text()
                .await
                .context("failed to access text content of playlist")?;

            if !playlist.contains("AccessDenied") {
                break playlist;
            }

            if retry_count >= 2 {
                anyhow::bail!("couldn't get access to playlist after 2 tries!");
            }

            println!("retrying playlist retrieval");
            retry_count += 1;
        };

        let key = self
            .get_key()
            .await
            .context("failed to get decryption key")?;

        let primary: crate::mp3url::M3U = playlist
            .parse()
            .context("could not parse primary playlist")?;

        let secondaries = self.get_secondary_playlists(primary.clone()).await;

        let primary = Primary::from_m3u(primary);
        // TODO explicitly find the correct playlist, instead of assuming index
        let secondary = Secondary::from_m3u(secondaries[usize::from(hq)].clone()); 
        let video_list = SchoolismVideoList::from_manifests(primary, secondary, key.0);

        Ok(video_list)
    }

    // should be done after navigating to a lesson
    async fn get_key(&self) -> anyhow::Result<Key> {
        let keytime_resp = self
            .net_client
            .get("https://www.schoolism.com/video-html/key-time.php")
            .send()
            .await
            .context("failed to send request to key-time")?;

        if !keytime_resp.status().is_success() {
            anyhow::bail!("failed to access key-time: [{:?}]", keytime_resp);
        }

        let key_resp = self
            .net_client
            .get("https://www.schoolism.com/video-html/key.php")
            .send()
            .await
            .context("failed to send request to access key response")?;

        if !key_resp.status().is_success() {
            anyhow::bail!("failed to access key: [{:?}]", key_resp);
        }

        let key = key_resp
            .bytes()
            .await
            .context("failed to access bytes of key response")?
            .to_vec();

        Ok(Key(key))
    }

    async fn get_secondary_playlists(&self, playlist: M3U) -> Vec<M3U> {
        let secondary_urls = playlist.subplaylists.into_iter().map(|it| it.url);
        let results: Vec<JoinHandle<Result<M3U>>> = secondary_urls
            .map(|it| {
                let net_client = self.net_client.clone();
                tokio::spawn(async move {
                    net_client
                        .get(&it)
                        .send()
                        .await
                        .context("could not get secondary playlist")?
                        .text()
                        .await
                        .context("could not get secondary playlist text")?
                        .parse()
                })
            })
            .collect();

        let (succ, _fail): (Vec<_>, Vec<_>) = futures::future::join_all(results)
            .await
            .into_iter()
            .partition(Result::is_ok);

        succ.into_iter()
            .flat_map(Result::unwrap)
            .collect()
    }
}

fn retrieve_base_url_for_playlist(playlist_url: &str) -> anyhow::Result<String> {
    let mut url = playlist_url
        .rsplitn(2, "/")
        .nth(1)
        .context(format!("invalid url to split: [{}]", playlist_url))?
        .to_owned();

    url.push('/');
    Ok(url)
}

pub struct SchoolismVideoList {
    pub key: Vec<u8>,
    pub iv: Vec<u8>,
    pub files: Vec<String>, // full url including domain
}

impl SchoolismVideoList {
    fn from_manifests(primary: Primary, secondary: Secondary, key: Vec<u8>) -> Self {
        let files = secondary
            .files
            .into_iter()
            .map(|it| {
                format!(
                    "{}{}",
                    retrieve_base_url_for_playlist(primary.streams.first().unwrap()).unwrap(),
                    it,
                )
            })
            .collect();

        Self {
            key,
            iv: secondary.iv,
            files, //
        }
    }
}

struct Primary {
    streams: Vec<String>,
}
impl Primary {
    fn from_m3u(m3u: M3U) -> Self {
        let streams = m3u.subplaylists.into_iter().map(|it| it.url).collect();
        Self { streams }
    }
}

struct Secondary {
    iv: Vec<u8>,
    files: Vec<String>,
}

impl Secondary {
    fn from_m3u(m3u: M3U) -> Self {
        let iv = m3u.key_info.unwrap().iv.clone();
        let iv = crate::util::decode_hex(&iv).unwrap();
        let files = m3u.tracklist.into_iter().map(|it| it.name).collect();
        Self { iv, files }
    }
}
