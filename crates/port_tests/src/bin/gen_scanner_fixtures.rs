//! Generate test QR images for scanner testing.
//! Creates SVGs in web/public/test-images/

use part_registry_codec::svg::{render_horz, render_vert};
use part_registry_codec::TextFormat;
use std::fs;

fn main() {
    let ids = [
        "23456789ABCDEF",
        "2345ABCDEFGHJK",
        "3456ABCDEFGHJK",
        "456789ABCDEFGH",
        "56789ABCDEFGHJ",
    ];

    let out_dir = "web/public/test-images";
    fs::create_dir_all(out_dir).unwrap();

    // Single QR
    let svg = render_horz(ids[0], 20.0, TextFormat::FiveFiveFour, false);
    let single = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 500 300" width="500" height="300">
<rect width="500" height="300" fill="#f5f5f5"/>
<g transform="translate(100,50) scale(4)">{svg}</g>
</svg>"##
    );
    fs::write(format!("{out_dir}/single-qr.svg"), &single).unwrap();

    // Multi QR grid (2x2)
    let mut grid = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 500" width="600" height="500">
<rect width="600" height="500" fill="#f0f0f0"/>"##,
    );
    for (i, id) in ids[..4].iter().enumerate() {
        let label = render_horz(id, 12.0, TextFormat::FourFourFour, false);
        let x = (i % 2) * 280 + 30;
        let y = (i / 2) * 220 + 30;
        grid.push_str(&format!(
            r##"<g transform="translate({x},{y}) scale(3)">{label}</g>"##
        ));
    }
    grid.push_str("</svg>");
    fs::write(format!("{out_dir}/multi-qr-grid.svg"), &grid).unwrap();

    // Multi QR scattered (random positions + rotations)
    let positions: [(i32, i32, i32); 5] = [
        (40, 30, -5), (320, 60, 8), (150, 250, -3), (400, 300, 12), (50, 380, -8),
    ];
    let mut scattered = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 650 550" width="650" height="550">
<rect width="650" height="550" fill="#e8e8e8"/>"##,
    );
    for (i, id) in ids.iter().enumerate() {
        let label = render_vert(id, 10.0, TextFormat::FourFourFour, true);
        let (x, y, rot) = positions[i];
        scattered.push_str(&format!(
            r##"<g transform="translate({x},{y}) rotate({rot}) scale(3)">{label}</g>"##
        ));
    }
    scattered.push_str("</svg>");
    fs::write(format!("{out_dir}/multi-qr-scattered.svg"), &scattered).unwrap();

    println!("Generated: {out_dir}/single-qr.svg, multi-qr-grid.svg, multi-qr-scattered.svg");
    println!("IDs: {:?}", ids);
}
