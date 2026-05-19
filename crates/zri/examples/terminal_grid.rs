use zri::render::{
    Color, Frame, Layer, PaintContext, Point, Rect, Renderer, Size, Stroke, TerminalGrid,
    TerminalRenderer, TextStyle,
};

fn main() {
    let frame = demo_frame();
    let mut renderer = TerminalRenderer::new(40, 12);
    let result = renderer.render(&frame).expect("terminal render failed");

    println!("zri terminal projection demo");
    println!(
        "rendered: {}, unsupported: {}",
        result.rendered_primitives, result.unsupported_primitives
    );
    println!();
    print_grid(renderer.grid());
}

fn demo_frame() -> Frame {
    let mut paint = PaintContext::new();
    paint.clear_color(Color::BLACK);
    paint.fill_rect(Rect::from_xywh(2.0, 2.0, 28.0, 7.0), Color::rgb(0, 0, 160));

    paint.with_layer(Layer(1), |paint| {
        paint.stroke_rect(
            Rect::from_xywh(5.0, 1.0, 30.0, 9.0),
            Stroke::new(Color::rgb(255, 190, 120), 1.0),
        );
        paint.text(
            Point::new(8.0, 4.0),
            "zri terminal grid",
            TextStyle::new(Color::WHITE, 12.0),
        );
        paint.line(
            Point::new(8.0, 7.0),
            Point::new(28.0, 7.0),
            Stroke::new(Color::rgb(255, 190, 120), 1.0),
        );
    });

    paint.with_clip(Rect::from_xywh(12.0, 5.0, 16.0, 3.0), |paint| {
        paint.with_layer(Layer(2), |paint| {
            paint.fill_rect(
                Rect::from_xywh(10.0, 4.0, 20.0, 5.0),
                Color::rgb(255, 255, 255),
            );
            paint.text(
                Point::new(13.0, 6.0),
                "clipped",
                TextStyle::new(Color::rgb(255, 0, 0), 12.0),
            );
        });
    });

    Frame::new(Size::new(40.0, 12.0), paint.finish())
}

fn print_grid(grid: &TerminalGrid) {
    for row in 0..grid.height {
        for col in 0..grid.width {
            let cell = grid.cell(col, row).expect("cell in bounds");
            if cell.ch == ' ' && cell.bg != Color::Transparent && cell.bg != Color::BLACK {
                print!("#");
            } else {
                print!("{}", cell.ch);
            }
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zri::render::Renderer;

    #[test]
    fn demo_frame_projects_to_terminal_grid() {
        let mut renderer = TerminalRenderer::new(40, 12);
        let result = renderer.render(&demo_frame()).unwrap();

        assert_eq!(result.unsupported_primitives, 0);
        assert!(result.rendered_primitives > 0);
        assert_eq!(renderer.grid().width, 40);
        assert_eq!(renderer.grid().height, 12);
    }
}
