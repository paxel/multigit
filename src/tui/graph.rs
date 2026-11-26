//! The Graph tab in the terminal: the condensed graph of all branches,
//! drawn with box-drawing characters.

use super::repo::{Pick, ViewRow};
use crate::app::RepoView;
use crate::domain::details::{Details, DiffTarget};
use crate::domain::graph::{GraphRow, LabelKind, RowKind, graph_rows, labels};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// The color of a lane.
pub fn lane_color(lane: usize) -> Color {
    const COLORS: [Color; 6] = [
        Color::Cyan,
        Color::Magenta,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Red,
    ];
    COLORS
        .get(lane % COLORS.len())
        .copied()
        .unwrap_or(Color::White)
}

fn put(cells: &mut [char], at: usize, c: char) {
    if let Some(cell) = cells.get_mut(at) {
        *cell = c;
    }
}

/// A line from the dot in lane `dot` to lane `lane`: a corner there and a
/// run between, crossing the lanes it passes.
fn bend(cells: &mut [char], dot: usize, lane: usize, from_above: bool) {
    let corner = match (lane > dot, from_above) {
        (true, true) => '\u{256f}',
        (false, true) => '\u{2570}',
        (true, false) => '\u{256e}',
        (false, false) => '\u{256d}',
    };
    put(cells, lane * 2, corner);
    let (from, to) = if lane > dot {
        (dot * 2 + 1, lane * 2)
    } else {
        (lane * 2 + 1, dot * 2)
    };
    for at in from..to {
        let crossed = match cells.get(at) {
            Some('\u{2502}') => '\u{253c}',
            _ => '\u{2500}',
        };
        put(cells, at, crossed);
    }
}

/// The lines of a row in `lanes` lanes, two cells each: the lane's line or
/// dot, then what runs to the right of it.
pub fn gutter(entry: &GraphRow, lanes: usize) -> String {
    let mut cells = vec![' '; lanes * 2];
    for &lane in &entry.through {
        put(&mut cells, lane * 2, '\u{2502}');
    }
    let dot = entry.lane;
    for &lane in &entry.into {
        bend(&mut cells, dot, lane, true);
    }
    for &lane in &entry.out {
        bend(&mut cells, dot, lane, false);
    }
    let mark = match &entry.kind {
        RowKind::Commit { merge: true, .. } => '\u{25cb}',
        RowKind::Commit { .. } => '\u{25cf}',
        RowKind::Fold { .. } => '\u{2506}',
    };
    put(&mut cells, dot * 2, mark);
    cells.into_iter().collect()
}

/// The rows of the Graph tab: the notes above it, then the graph.
pub fn rows(details: &Details, view: &RepoView) -> Vec<ViewRow> {
    let graph = match &view.graph {
        None => {
            return vec![ViewRow {
                line: Line::styled("loading", dim()),
                pick: Pick::Nothing,
            }];
        }
        Some(Err(error)) => {
            return vec![ViewRow {
                line: Line::styled(error.clone(), Style::new().fg(Color::Red)),
                pick: Pick::Nothing,
            }];
        }
        Some(Ok(graph)) => graph,
    };
    let mut out = Vec::new();
    if graph.hidden > 0 && !view.with_old {
        let text = if graph.hidden == 1 {
            "1 old remote branch hidden (Enter shows it)".to_owned()
        } else {
            format!(
                "{} old remote branches hidden (Enter shows them)",
                graph.hidden
            )
        };
        out.push(ViewRow {
            line: Line::styled(text, Style::new().fg(Color::Blue)),
            pick: Pick::Old,
        });
    }
    if graph.cut {
        out.push(ViewRow {
            line: Line::styled("older history is not shown", dim()),
            pick: Pick::Nothing,
        });
    }
    let head = details.branches.iter().find(|b| b.current);
    let names = labels(&graph.refs, head.map(|b| b.name.as_str()));
    let rows = graph_rows(&graph.commits, &names, &view.graph_open);
    if rows.is_empty() {
        out.push(ViewRow {
            line: Line::styled("no commits yet", dim()),
            pick: Pick::Nothing,
        });
        return out;
    }
    let lanes = rows.iter().map(GraphRow::width).max().unwrap_or(1);
    for entry in rows {
        let mut spans = vec![Span::styled(
            gutter(&entry, lanes),
            Style::new().fg(lane_color(entry.lane)),
        )];
        let pick = match &entry.kind {
            RowKind::Fold { count, first } => {
                spans.push(Span::styled(format!("{count} commits"), dim()));
                Pick::Unfold(first.clone())
            }
            RowKind::Commit {
                hash,
                subject,
                labels,
                ..
            } => {
                for label in labels {
                    let color = match label.kind {
                        LabelKind::Local | LabelKind::Both => lane_color(entry.lane),
                        LabelKind::Remote => Color::DarkGray,
                        LabelKind::Tag => Color::Yellow,
                    };
                    let mut style = Style::new().fg(color);
                    if label.head {
                        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                    }
                    spans.push(Span::styled(format!("[{}]", label.text), style));
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::raw(subject.clone()));
                Pick::Select(DiffTarget::Commit(hash.clone()))
            }
        };
        out.push(ViewRow {
            line: Line::from(spans),
            pick,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(lane: usize, through: &[usize], into: &[usize], out: &[usize]) -> GraphRow {
        GraphRow {
            lane,
            up: true,
            down: true,
            through: through.to_vec(),
            into: into.to_vec(),
            out: out.to_vec(),
            kind: RowKind::Commit {
                hash: "a".into(),
                author: "A".into(),
                subject: "s".into(),
                labels: Vec::new(),
                merge: false,
            },
        }
    }

    #[test]
    fn lanes_pass_by_and_bend_into_the_dot() {
        assert_eq!(gutter(&row(0, &[1], &[], &[]), 2), "\u{25cf} \u{2502} ");
        assert_eq!(
            gutter(&row(0, &[], &[1], &[]), 2),
            "\u{25cf}\u{2500}\u{256f} "
        );
        assert_eq!(
            gutter(&row(1, &[], &[], &[0]), 2),
            "\u{256d}\u{2500}\u{25cf} "
        );
        assert_eq!(
            gutter(&row(0, &[1], &[2], &[]), 3),
            "\u{25cf}\u{2500}\u{253c}\u{2500}\u{256f} "
        );
    }
}
