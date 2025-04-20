let TASKS = {};
let INVALID_EVENTS = [];

let W, H;
let CANVAS;
let GCTX;
let SCALE = 10000;
let SCALE_DELTA = null;
let OFFSET_DELTA = null;
let POINTER_PX_OFFSET = 0;

const LEFT_COLUMN = 250;
let TIME_OFFSET = 0;
let FULL_REPAINT = true;
let OFFLINE = false;

function line(x1, y1, x2, y2) {
    GCTX.beginPath();
    GCTX.moveTo(x1, y1);
    GCTX.lineTo(x2, y2);
    GCTX.stroke();
}

function draw_step(x, y, width, left_to_right) {
    if (x < LEFT_COLUMN || x > W) return;
    let x2 = left_to_right ? (x + width) : (x - width);
    GCTX.beginPath();
    GCTX.moveTo(x, y - 6);
    GCTX.lineTo(x2, y);
    GCTX.lineTo(x, y + 6);
    GCTX.lineTo(x, y - 6);
    GCTX.fill();
    line(x2, y - 6, x2, y + 6);
}

function task_y(id) {
    return 20 + 40 * id;
}

function time_scale(timestamp) {
    return timestamp / SCALE;
}

function time_to_px(timestamp) {
    let offset = LEFT_COLUMN + time_scale(timestamp - TIME_OFFSET);

    if (offset > W && !OFFLINE) {
        TIME_OFFSET += (W / 2) * SCALE;
        FULL_REPAINT = true;
    }

    return (offset < LEFT_COLUMN) ? -100 : offset;
}

function draw_poll(id, poll) {
    let health = (1000 - poll.duration) / 10;
    if (health < 0) health = 0;
    GCTX.fillStyle = 'rgb(100%, ' + health + '%, ' + health + '%)';

    let y = task_y(id);
    let x = time_to_px(poll.start);

    let width = time_scale(poll.duration);
    if (!poll.is_done) x += width;
    draw_step(x, y, width, !poll.is_done);
}

function draw_task_name(id, name) {
    let y = task_y(id);

    GCTX.strokeStyle = 'white';
    line(0, y + 20, W, y + 20);

    GCTX.fillStyle = 'white';
    GCTX.fillText(name, 15, y, LEFT_COLUMN);
}

function draw_task_start(id, start_time) {
    let y = task_y(id);
    let x = time_to_px(start_time);

    GCTX.strokeStyle = 
    line(x, y - 20, x, y + 20);
    GCTX.fillStyle = 'green';
    GCTX.fillRect(x - 1, y - 20, 1, 40);
}

function tick() {
    W = window.innerWidth;
    H = window.innerHeight;

    if (W != CANVAS.width || H != CANVAS.height || FULL_REPAINT) {
        CANVAS.width = W;
        CANVAS.height = H;
        GCTX.font = '16px sans-serif';
        FULL_REPAINT = true;
    }

    if (SCALE_DELTA !== null) {
        let pointer_time_offset = TIME_OFFSET + (POINTER_PX_OFFSET - LEFT_COLUMN) * SCALE;
        SCALE *= SCALE_DELTA;
        TIME_OFFSET = pointer_time_offset - (SCALE * (POINTER_PX_OFFSET - LEFT_COLUMN));
        SCALE_DELTA = null;
        FULL_REPAINT = true;
    }

    if (OFFSET_DELTA !== null) {
        TIME_OFFSET += OFFSET_DELTA * SCALE;
        OFFSET_DELTA = null;
        FULL_REPAINT = true;
    }

    if (FULL_REPAINT) {
        GCTX.clearRect(0, 0, window.innerWidth, window.innerHeight);
        GCTX.strokeStyle = 'white';
        GCTX.textBaseline = 'middle';
        line(LEFT_COLUMN, 0, LEFT_COLUMN, H);

        for (let id in TASKS) {
            let task = TASKS[id];
            draw_task_name(id, task.name);

            let polls = task.polls;
            for (let i = 0; i < polls.length; i++) {
                let poll = polls[i];
                if (i == 0) draw_task_start(id, poll.start);
                draw_poll(id, poll);
            }
        }

        FULL_REPAINT = false;
    }

    if (!OFFLINE) {
        // MAKE HTTP REQUEST
        let req = new XMLHttpRequest();
        req.addEventListener('load', on_update_load);
        req.addEventListener('error', on_update_error);
        req.responseType = 'json';
        req.open('GET', '/update.json');
        req.send();
    } else {
        setTimeout(tick, 100);
    }
}

function on_update_load() {
    let new_tasks = this.response.new_tasks;
    for (let i = 0; i < new_tasks.length; i++) {
        let task = new_tasks[i];

        TASKS[task.id] = {
            name: task.name,
            runner: task.runner,
            last_event: null,
            polls: [],
        };

        draw_task_name(task.id, task.name);
    }

    let events = this.response.task_events;
    let retried = INVALID_EVENTS.splice(0);
    events.push(...retried);

    for (let i = 0; i < events.length; i++) {
        let event = events[i];
        let task = TASKS[event.id];

        if (task === undefined) {
            INVALID_EVENTS.push(event);
            continue;
        }

        // console.log('task ' + task.name + ': ' + event.type);

        if (task.last_event === null) {
            task.last_event = event;
            continue;
        }

        let poll_start = task.last_event;
        task.last_event = null;

        if (poll_start.type !== 'POLLING') {
            console.error('bad polling event');
        }

        let start = poll_start.timestamp;
        let duration = event.timestamp - start;
        let is_done = event.type === 'POLL_READY';

        let poll = {
            start,
            duration,
            is_done,
        };

        if (task.polls.length == 0) {
            draw_task_start(event.id, poll.start);
        }

        draw_poll(event.id, poll);
        task.polls.push(poll);
    }

    setTimeout(tick, 100);
}

function on_update_error() {
    OFFLINE = true;
    setTimeout(tick, 100);
}

function on_wheel(event) {
    if (event.shiftKey && OFFLINE) {
        OFFSET_DELTA = (event.deltaY > 0) ? -200 : 200;
    } else {
        POINTER_PX_OFFSET = event.clientX;
        let delta = 3/4;
        if (event.deltaY > 0) delta = 1 / delta;
        SCALE_DELTA = delta;
    }
}

function init() {
    CANVAS = find('canvas');
    GCTX = CANVAS.getContext('2d');

    addEventListener("wheel", on_wheel);

    tick();
}
