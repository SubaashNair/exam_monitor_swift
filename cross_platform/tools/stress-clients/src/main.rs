use anyhow::{anyhow, Context, Result};
use exam_monitor_core::protocol::{identity_payload, write_packet, PacketType};
use image::codecs::jpeg::JpegEncoder;
use image::{ColorType, Rgb, RgbImage};
use std::env;
use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct Config {
    host: String,
    port: u16,
    clients: usize,
    fps: u64,
    seconds: u64,
    width: u32,
    height: u32,
    quality: u8,
    code: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: String::from("127.0.0.1"),
            port: 1234,
            clients: 20,
            fps: 5,
            seconds: 60,
            width: 720,
            height: 405,
            quality: 60,
            code: String::new(),
        }
    }
}

#[derive(Debug)]
struct ClientResult {
    client_index: usize,
    frames_sent: u64,
    bytes_sent: u64,
}

fn main() -> Result<()> {
    let config = Config::from_args(env::args().skip(1))?;
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .context("host and port must form a valid socket address")?;

    println!(
        "Starting {} synthetic clients -> {addr}, {} FPS, {}s, {}x{}, JPEG quality {}",
        config.clients, config.fps, config.seconds, config.width, config.height, config.quality
    );

    let started_at = Instant::now();
    let mut workers = Vec::with_capacity(config.clients);

    for client_index in 1..=config.clients {
        let config = config.clone();
        workers.push(thread::spawn(move || {
            run_client(client_index, addr, config)
        }));
    }

    let mut total_frames = 0_u64;
    let mut total_bytes = 0_u64;

    for worker in workers {
        let result = worker
            .join()
            .map_err(|_| anyhow!("a synthetic client thread panicked"))??;

        println!(
            "client {:02}: sent {} frames, {:.2} MB",
            result.client_index,
            result.frames_sent,
            result.bytes_sent as f64 / 1_000_000.0
        );

        total_frames += result.frames_sent;
        total_bytes += result.bytes_sent;
    }

    let elapsed = started_at.elapsed().as_secs_f64();
    println!(
        "total: {total_frames} frames, {:.2} MB, {:.2} FPS aggregate, {:.2} Mbps",
        total_bytes as f64 / 1_000_000.0,
        total_frames as f64 / elapsed,
        (total_bytes as f64 * 8.0 / elapsed) / 1_000_000.0
    );

    Ok(())
}

fn run_client(client_index: usize, addr: SocketAddr, config: Config) -> Result<ClientResult> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .with_context(|| format!("client {client_index:02} failed to connect to {addr}"))?;
    stream.set_nodelay(true).ok();
    stream.set_write_timeout(Some(Duration::from_secs(3))).ok();

    let student_name = format!("Synthetic Student {client_index:02}");
    let student_id = format!("SYN-{client_index:02}");
    write_packet(
        &mut stream,
        PacketType::Name,
        &identity_payload(&config.code, &student_name, &student_id),
    )
    .with_context(|| format!("client {client_index:02} failed to send identity"))?;

    let frame_interval = Duration::from_millis(1_000 / config.fps.max(1));
    let stop_at = Instant::now() + Duration::from_secs(config.seconds);
    let mut frame_index = 0_u64;
    let mut bytes_sent = 0_u64;

    while Instant::now() < stop_at {
        let started_at = Instant::now();
        let frame = synthetic_jpeg_frame(
            client_index,
            frame_index,
            config.width,
            config.height,
            config.quality,
        )?;

        write_packet(&mut stream, PacketType::Picture, &frame)
            .with_context(|| format!("client {client_index:02} failed to send frame"))?;

        bytes_sent += frame.len() as u64;
        frame_index += 1;

        if let Some(remaining) = frame_interval.checked_sub(started_at.elapsed()) {
            thread::sleep(remaining);
        }
    }

    Ok(ClientResult {
        client_index,
        frames_sent: frame_index,
        bytes_sent,
    })
}

fn synthetic_jpeg_frame(
    client_index: usize,
    frame_index: u64,
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>> {
    let mut image = RgbImage::new(width, height);
    let seed = client_index as u32 * 37 + frame_index as u32 * 13;

    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let block = ((x / 48) + (y / 36) + seed) % 8;
        let motion = ((x + frame_index as u32 * 9) % width.max(1)) < width.max(1) / 4;
        let base = if motion { 180_u8 } else { 40_u8 };

        *pixel = Rgb([
            base.saturating_add(((block * 17 + seed) % 70) as u8),
            50_u8.saturating_add(((x + seed) % 130) as u8),
            70_u8.saturating_add(((y + seed * 2) % 110) as u8),
        ]);
    }

    draw_marker(&mut image, client_index as u32, frame_index as u32);

    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, quality);
    encoder.encode(image.as_raw(), width, height, ColorType::Rgb8)?;
    Ok(bytes)
}

fn draw_marker(image: &mut RgbImage, client_index: u32, frame_index: u32) {
    let width = image.width().max(1);
    let height = image.height().max(1);
    let marker_width = (width / 5).max(80);
    let marker_height = (height / 8).max(36);
    let x0 = ((frame_index * 11) % width).min(width.saturating_sub(marker_width));
    let y0 = ((client_index * 23) % height).min(height.saturating_sub(marker_height));

    for y in y0..(y0 + marker_height).min(height) {
        for x in x0..(x0 + marker_width).min(width) {
            let border = y == y0
                || x == x0
                || y == (y0 + marker_height).min(height) - 1
                || x == (x0 + marker_width).min(width) - 1;
            let color = if border {
                Rgb([255, 255, 255])
            } else {
                Rgb([
                    (client_index * 31 % 255) as u8,
                    (frame_index * 19 % 255) as u8,
                    220,
                ])
            };
            image.put_pixel(x, y, color);
        }
    }
}

impl Config {
    fn from_args(args: impl Iterator<Item = String>) -> Result<Self> {
        let mut config = Self::default();
        let mut args = args.peekable();

        while let Some(arg) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| anyhow!("missing value after {arg}"))
            };

            match arg.as_str() {
                "--host" => config.host = value()?,
                "--port" => config.port = value()?.parse().context("--port must be a u16")?,
                "--clients" => {
                    config.clients = value()?.parse().context("--clients must be a number")?
                }
                "--fps" => config.fps = value()?.parse().context("--fps must be a number")?,
                "--seconds" => {
                    config.seconds = value()?.parse().context("--seconds must be a number")?
                }
                "--width" => config.width = value()?.parse().context("--width must be a number")?,
                "--height" => {
                    config.height = value()?.parse().context("--height must be a number")?
                }
                "--quality" => {
                    config.quality = value()?.parse().context("--quality must be a number")?
                }
                "--code" => config.code = value()?,
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                _ => return Err(anyhow!("unknown argument: {arg}")),
            }
        }

        if config.clients == 0 {
            return Err(anyhow!("--clients must be greater than zero"));
        }
        if config.fps == 0 {
            return Err(anyhow!("--fps must be greater than zero"));
        }
        if config.width == 0 || config.height == 0 {
            return Err(anyhow!("--width and --height must be greater than zero"));
        }

        Ok(config)
    }
}

fn print_usage() {
    println!(
        "Usage: cargo run -p stress-clients -- --host 127.0.0.1 --port 1234 --clients 20 --fps 5 --seconds 60"
    );
}
