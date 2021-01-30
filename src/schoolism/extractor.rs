use anyhow::Context;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};

static VIDEO_LIST_START_RE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"allVideos").unwrap());

// no backreferences :V
static PLAYLIST_URL_RE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r#"src\s*:\s*["'](https://[^"']+)["']\s*,"#).unwrap());

pub fn parse_dashboard(page: &str) -> anyhow::Result<Vec<super::Lesson>> {
    let document = Html::parse_document(page);

    let main_selector = Selector::parse("div.mainContentArea")
        .map_err(|e| anyhow::anyhow!("{:?}", e))
        .context("could not create main selector for html")?;

    let lesson_selector = Selector::parse("div.clearfix > div.greyButton > a")
        .map_err(|e| anyhow::anyhow!("{:?}", e))
        .context("could not create lesson selector for html")?;

    let main = document
        .select(&main_selector)
        .nth(0)
        .context("could not find main content area in dashboard response")?;

    let links = main.select(&lesson_selector);

    let (links, errors): (Vec<_>, Vec<_>) = links
        .map(|l| l.value().attr("href"))
        .partition(Option::is_some);

    if errors.len() > 0 {
        let msg = format!(
            "[{}] errors found while parsing dashboard links",
            errors.len()
        );
        println!("{}", msg);
    }

    let ret: Vec<_> = links
        .into_iter()
        .map(Option::unwrap)
        .filter(|&link| link.starts_with("watchLesson.php"))
        .enumerate()
        .map(|(no, link)| super::Lesson {
            _no: no,
            link: link.into(),
        })
        .collect();

    Ok(ret)
}

pub fn parse_lesson(page: &str) -> anyhow::Result<Vec<super::LessonPart>> {
    // find the allVideos js array, and map only the the "src" field
    // assume the urls are sorted
    let captures = VIDEO_LIST_START_RE
        .find(page)
        .context("can't find video list on page response")?;
    let i = captures.start();
    let narrow = &page[i..];
    let video_list = crate::util::matching_bracket_substring(narrow, '[')
        .context("failed to find matching bracket substring for video url list")?;

    let urls: Vec<_> = PLAYLIST_URL_RE
        .captures_iter(video_list)
        .map(|it| super::LessonPart { url: it[1].into() })
        .collect();

    Ok(urls)
}
