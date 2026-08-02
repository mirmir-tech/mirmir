mod data;

use std::{env, fmt::Write as _, fs, path::Path};

use data::demo_app;
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::{Buffer, Cell},
    style::{Color, Modifier},
};

use super::super::render;

const WIDTH: u16 = 160;
const HEIGHT: u16 = 45;

#[test]
#[ignore = "writes a browser-renderable capture fixture"]
fn writes_dashboard_capture() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::var("MIRMIR_TUI_CAPTURE").or_else(|_| env::var("MIRMIR_TUI_CAPTURE_HTML"))?;
    let view = env::var("MIRMIR_TUI_CAPTURE_VIEW").unwrap_or_else(|_| "dashboard".to_owned());
    let mut app = demo_app(&view);
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| render::draw(frame, &mut app))?;
    let buffer = terminal.backend().buffer();
    let capture = if Path::new(&output)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        capture_svg(buffer)?
    } else {
        capture_html(buffer)?
    };
    fs::write(output, capture)?;
    Ok(())
}

fn capture_html(buffer: &Buffer) -> Result<String, std::fmt::Error> {
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><style>\
         *{box-sizing:border-box}html,body{margin:0;width:1600px;height:900px;overflow:hidden;\
         background:#0d1117}main{display:grid;grid-template-columns:repeat(160,10px);\
         grid-template-rows:repeat(45,20px);width:1600px;height:900px;\
         font:16px/20px 'JetBrains Mono',Menlo,monospace;font-variant-ligatures:none;\
         -webkit-font-smoothing:antialiased}span{display:block;width:10px;height:20px;overflow:visible;\
         white-space:pre}.braille{position:relative}.braille i{position:absolute;width:2px;height:2px;\
         border-radius:50%;background:currentColor;transform:translate(-50%,-50%)}</style><main>",
    );
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let cell = buffer.cell((x, y)).expect("capture coordinates must be in bounds");
            write!(
                html,
                "<span{} style=\"{}\">{}</span>",
                cell_class(cell.symbol()),
                cell_style(cell),
                cell_contents(cell.symbol())?
            )?;
        }
    }
    html.push_str("</main>");
    Ok(html)
}

fn capture_svg(buffer: &Buffer) -> Result<String, std::fmt::Error> {
    let mut svg = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1600\" height=\"900\" \
         viewBox=\"0 0 1600 900\"><rect width=\"1600\" height=\"900\" fill=\"#0d1117\"/>\
         <g font-family=\"JetBrains Mono,Menlo,monospace\" font-size=\"16\" \
         text-rendering=\"geometricPrecision\">",
    );
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let cell = buffer.cell((x, y)).expect("capture coordinates must be in bounds");
            svg_cell(&mut svg, cell, x, y)?;
        }
    }
    svg.push_str("</g></svg>");
    Ok(svg)
}

fn svg_cell(svg: &mut String, cell: &Cell, x: u16, y: u16) -> Result<(), std::fmt::Error> {
    let left = u32::from(x) * 10;
    let top = u32::from(y) * 20;
    let foreground = foreground(cell.fg);
    write!(
        svg,
        "<rect x=\"{left}\" y=\"{top}\" width=\"10\" height=\"20\" fill=\"{}\"/>",
        background(cell.bg)
    )?;
    if let Some(bits) = braille_bits(cell.symbol()) {
        for (bit, dot_x, dot_y) in [
            (0, 25, 25),
            (1, 25, 75),
            (2, 25, 125),
            (3, 75, 25),
            (4, 75, 75),
            (5, 75, 125),
            (6, 25, 175),
            (7, 75, 175),
        ] {
            if bits & (1 << bit) != 0 {
                write!(
                    svg,
                    "<circle cx=\"{}.5\" cy=\"{}.5\" r=\"1\" fill=\"{foreground}\"/>",
                    left + dot_x / 10,
                    top + dot_y / 10
                )?;
            }
        }
    } else if !cell.symbol().trim().is_empty() {
        let weight = if cell.modifier.contains(Modifier::BOLD) {
            " font-weight=\"700\""
        } else {
            ""
        };
        let opacity = if cell.modifier.contains(Modifier::DIM) {
            " opacity=\".65\""
        } else {
            ""
        };
        write!(
            svg,
            "<text x=\"{left}\" y=\"{}\" fill=\"{foreground}\"{weight}{opacity}>{}</text>",
            top + 16,
            escape(cell.symbol())
        )?;
    }
    Ok(())
}

fn cell_style(cell: &Cell) -> String {
    let mut style = format!("color:{};background:{}", foreground(cell.fg), background(cell.bg));
    if cell.modifier.contains(Modifier::BOLD) {
        style.push_str(";font-weight:700");
    }
    if cell.modifier.contains(Modifier::DIM) {
        style.push_str(";opacity:.65");
    }
    style
}

fn foreground(color: Color) -> String {
    color_value(color, "#e7ecef")
}

fn background(color: Color) -> String {
    color_value(color, "#0d1117")
}

fn color_value(color: Color, reset: &str) -> String {
    match color {
        Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
        Color::Reset => reset.to_owned(),
        other => other.to_string(),
    }
}

fn escape(symbol: &str) -> String {
    symbol.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn cell_class(symbol: &str) -> &'static str {
    if braille_bits(symbol).is_some() {
        " class=\"braille\""
    } else {
        ""
    }
}

fn cell_contents(symbol: &str) -> Result<String, std::fmt::Error> {
    let Some(bits) = braille_bits(symbol) else {
        return Ok(escape(symbol));
    };
    let mut dots = String::new();
    for (bit, left, top) in [
        (0, 25, 125),
        (1, 25, 375),
        (2, 25, 625),
        (3, 75, 125),
        (4, 75, 375),
        (5, 75, 625),
        (6, 25, 875),
        (7, 75, 875),
    ] {
        if bits & (1 << bit) != 0 {
            write!(dots, "<i style=\"left:{left}%;top:{}%\"></i>", top / 10)?;
        }
    }
    Ok(dots)
}

fn braille_bits(symbol: &str) -> Option<u8> {
    let mut chars = symbol.chars();
    let character = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let codepoint = u32::from(character);
    let bits = codepoint.checked_sub(0x2800)?;
    u8::try_from(bits).ok()
}

#[test]
fn renders_braille_as_a_full_cell_dot_grid() -> Result<(), std::fmt::Error> {
    let dots = cell_contents("⣿")?;
    assert_eq!(dots.matches("<i ").count(), 8);
    assert!(dots.contains("left:25%;top:12%"));
    assert!(dots.contains("left:75%;top:87%"));
    assert_eq!(cell_contents("A")?, "A");
    Ok(())
}
