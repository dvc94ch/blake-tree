use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers};
use futures::stream::FuturesOrdered;
use futures::{FutureExt, StreamExt};
use peershare_core::StreamId;
use ratatui::{
    prelude::{CrosstermBackend, Modifier, Rect, Style, Terminal},
    widgets::Paragraph,
};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    pub hits: Vec<Hit>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hit {
    pub stream_id: StreamId,
    pub source: String,
}

struct Search {
    url: Url,
}

impl Search {
    pub fn new(mut url: Url, index: &str) -> Self {
        url.set_path(&format!("/indexes/{index}/search"));
        Self { url }
    }

    pub async fn query(&self, query: String, limit: u16) -> Result<Response> {
        let res = Client::new()
            .post(self.url.clone())
            .json(&json!({
                "q": query,
                "limit": limit,
            }))
            .send()
            .await?;
        if res.status() != StatusCode::OK {
            anyhow::bail!("received status code {}", res.status());
        }
        Ok(res.json().await?)
    }
}

#[derive(Default)]
struct State {
    query: String,
    limit: u16,
    hits: Vec<Hit>,
    select: usize,
}

impl State {
    pub fn push(&mut self, c: char) {
        self.query.push(c);
    }

    pub fn pop(&mut self) {
        self.query.pop();
    }

    pub fn set_limit(&mut self, limit: u16) {
        self.limit = limit;
    }

    pub fn set_hits(&mut self, hits: Vec<Hit>) {
        self.hits = hits;
        self.select = std::cmp::min(self.select, self.hits.len().saturating_sub(1));
    }

    pub fn up(&mut self) {
        self.select -= 1;
        self.select %= self.hits.len();
    }

    pub fn down(&mut self) {
        self.select += 1;
        self.select %= self.hits.len();
    }

    pub fn selection(&self) -> &Hit {
        &self.hits[self.select]
    }
}

async fn run(url: Url) -> Result<Option<StreamId>> {
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stderr()))?;

    let mut state = State::default();
    let mut events = EventStream::new();
    let search = Search::new(url, "content");
    let mut queries = FuturesOrdered::new();

    let start_query = |queries: &mut FuturesOrdered<_>, state: &State| {
        queries.push_back(search.query(state.query.clone(), state.limit));
    };

    loop {
        terminal.draw(|f| {
            let width = f.size().width;
            f.render_widget(
                Paragraph::new(format!("> {}", state.query)),
                Rect::new(0, 0, width, 1),
            );
            let start_y = 1;
            let hit_height = 1;
            state.set_limit((f.size().height - 1) / hit_height);
            for (i, hit) in state.hits.iter().enumerate() {
                let y = start_y + hit_height * (i as u16);
                let style = if state.select == i {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                f.render_widget(
                    Paragraph::new(hit.source.clone()).style(style),
                    Rect::new(0, y, width, 1),
                );
            }
        })?;

        futures::select! {
            event = events.next().fuse() => {
                let Some(event) = event else {
                    continue;
                };
                if let Event::Key(key) = event? {
                    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                        break;
                    }
                    match key.code {
                        KeyCode::Char(c) => {
                            state.push(c);
                            start_query(&mut queries, &state);
                        }
                        KeyCode::Backspace => {
                            state.pop();
                            start_query(&mut queries, &state);
                        }
                        KeyCode::Up => {
                            state.up();
                        }
                        KeyCode::Down | KeyCode::Tab => {
                            state.down();
                        }
                        KeyCode::Enter => {
                            return Ok(Some(state.selection().stream_id.clone()));
                        }
                        _ => continue,
                    }
                }
            }
            response = queries.next().fuse() => {
                let Some(response) = response else {
                    continue;
                };
                state.set_hits(response?.hits);
            }
        }
    }
    Ok(None)
}

pub async fn select_stream(url: Url) -> Result<Option<StreamId>> {
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(std::io::stderr(), crossterm::terminal::EnterAlternateScreen)?;

    let result = run(url).await;

    crossterm::execute!(std::io::stderr(), crossterm::terminal::LeaveAlternateScreen)?;
    crossterm::terminal::disable_raw_mode()?;

    result
}
