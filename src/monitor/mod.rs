use futures_lite::{AsyncReadExt, AsyncWriteExt};
use async_net::{TcpStream, TcpListener};
use async_fifo::non_blocking::Consumer;
use std::time::Instant;
use serde::Serialize;

use super::{TaskExec, TaskDecl, TaskId};

#[derive(Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
enum TaskEvent {
    Polling {
        id: TaskId,
        timestamp: u128,
    },
    PollReady {
        id: TaskId,
        timestamp: u128,
    },
    PollPending {
        id: TaskId,
        timestamp: u128,
    },
}

#[derive(Serialize)]
struct Update {
    new_tasks: Vec<TaskDecl>,
    task_events: Vec<TaskEvent>,
    current_time: u128,
}

fn create_update(
    time_ref: Instant,
    rx_exec: &Consumer<(TaskExec, Instant)>,
    rx_decl: &Consumer<TaskDecl>,
) -> Update {
    let mut new_tasks = Vec::new();
    rx_decl.try_recv_many(&mut new_tasks);

    let mut exec = Vec::new();
    rx_exec.try_recv_many(&mut exec);

    let conv = |(exec, instant): (_, Instant)| {
        let duration = instant.checked_duration_since(time_ref);
        let timestamp = duration.unwrap_or_default().as_micros();

        match exec {
            TaskExec::Polling(id) => TaskEvent::Polling { id, timestamp },
            TaskExec::PollReady(id) => TaskEvent::PollReady { id, timestamp },
            TaskExec::PollPending(id) => TaskEvent::PollPending { id, timestamp },
        }
    };

    let task_events = exec.drain(..).map(conv).collect();

    let duration = Instant::now().checked_duration_since(time_ref);
    let current_time = duration.unwrap_or_default().as_micros();

    Update {
        new_tasks,
        task_events,
        current_time,
    }
}

async fn get_req_path(stream: &mut TcpStream) -> Option<(String, Option<String>)> {
    let mut buffer = Vec::new();

    while buffer.windows(4).all(|w| w != b"\r\n\r\n") {
        let mut tmp = [0; 128];
        let len = stream.read(&mut tmp).await.ok()?;
        buffer.extend_from_slice(&tmp[..len]);
    }

    let request = String::from_utf8(buffer).ok()?;
    let path_and_headers = request.strip_prefix("GET ")?;
    let (path, headers) = path_and_headers.split_once(" HTTP/1.1\r\n")?;

    if let Some((_before, after)) = headers.split_once("Action: ") {
        let (value, _after) = after.split_once("\r\n")?;
        return Some((path.into(), Some(value.to_string())));
    }

    Some((path.into(), None))
}

pub async fn server(
    port: u16,
    rx_exec: Consumer<(TaskExec, Instant)>,
    rx_decl: Consumer<TaskDecl>,
) {
    let time_ref = Instant::now();
    let listener = TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    loop {
        let (mut stream, _addr) = listener.accept().await.unwrap();
        let Some((path, _maybe_action)) = get_req_path(&mut stream).await else {
            continue;
        };

        let mut json = String::new();

        if path == "/update.json" {
            let update = create_update(time_ref, &rx_exec, &rx_decl);
            json = serde_json::to_string(&update).unwrap();
        }

        // let _index_js = std::fs::read_to_string("src/frontend/index.js").unwrap();

        let (body, content_type) = match &*path {
            // "/index.js" => (&*_index_js, "text/javascript"),

            "/" => (include_str!("index.html"), "text/html"),
            "/index.css" => (include_str!("index.css"), "text/css"),
            "/utils.js" => (include_str!("utils.js"), "text/javascript"),
            "/index.js" => (include_str!("index.js"), "text/javascript"),
            "/update.json" => (json.as_str(), "application/json"),
            "/action" => ("OK", "text/html"),
            _other => ("NOT FOUND", "text/html"),
        };

        let len_hdr = format!("Content-Length: {}", body.len());
        let type_hdr = format!("Content-Type: {content_type}; charset=UTF-8");

        let lines = [
            "HTTP/1.1 200 OK",
            "Connection: close",
            &type_hdr,
            &len_hdr,
            "",
            body,
        ];

        let response = lines.join("\r\n");
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
    }
}
