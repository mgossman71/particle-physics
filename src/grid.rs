/// Uniform spatial hash: counting-sorts point indices into square cells.
/// Buffers are kept between frames so rebuilding allocates nothing.
#[derive(Default)]
pub struct Grid {
    cols: usize,
    rows: usize,
    cell: f64,
    start: Vec<usize>,
    cursor: Vec<usize>,
    cell_of: Vec<usize>,
    items: Vec<usize>,
}

impl Grid {
    pub fn rebuild(&mut self, cell: f64, w: f64, h: f64, points: impl Iterator<Item = (f64, f64)>) {
        self.cell = cell;
        self.cols = (w / cell) as usize + 1;
        self.rows = (h / cell) as usize + 1;
        let ncells = self.cols * self.rows;
        self.start.clear();
        self.start.resize(ncells + 1, 0);
        self.cell_of.clear();
        for (x, y) in points {
            let (cx, cy) = self.coords(x, y);
            let c = cy * self.cols + cx;
            self.cell_of.push(c);
            self.start[c + 1] += 1;
        }
        for c in 0..ncells { self.start[c + 1] += self.start[c]; }
        self.cursor.clear();
        self.cursor.extend_from_slice(&self.start[..ncells]);
        self.items.clear();
        self.items.resize(self.cell_of.len(), 0);
        for (i, &c) in self.cell_of.iter().enumerate() {
            self.items[self.cursor[c]] = i;
            self.cursor[c] += 1;
        }
    }

    /// Cell coordinates for a point, clamped into the grid (`as` saturates negatives to 0)
    fn coords(&self, x: f64, y: f64) -> (usize, usize) {
        (((x / self.cell) as usize).min(self.cols - 1), ((y / self.cell) as usize).min(self.rows - 1))
    }

    fn cell(&self, cx: usize, cy: usize) -> &[usize] {
        let c = cy * self.cols + cx;
        &self.items[self.start[c]..self.start[c + 1]]
    }

    /// Calls `f(i, j)` once for every pair of points in the same or adjacent cells.
    pub fn for_each_pair(&self, mut f: impl FnMut(usize, usize)) {
        for cy in 0..self.rows {
            for cx in 0..self.cols {
                let here = self.cell(cx, cy);
                for (k, &i) in here.iter().enumerate() {
                    for &j in &here[k + 1..] { f(i, j); }
                }
                // Forward neighbours only (E, SW, S, SE) so each pair is visited once
                for (ox, oy) in [(1isize, 0isize), (-1, 1), (0, 1), (1, 1)] {
                    let nx = cx as isize + ox;
                    let ny = cy as isize + oy;
                    if nx < 0 || nx >= self.cols as isize || ny >= self.rows as isize { continue; }
                    let there = self.cell(nx as usize, ny as usize);
                    for &i in here {
                        for &j in there { f(i, j); }
                    }
                }
            }
        }
    }

    /// Calls `f(i)` for every point in the 3×3 block of cells around (x, y).
    pub fn for_each_near(&self, x: f64, y: f64, mut f: impl FnMut(usize)) {
        let (cx, cy) = self.coords(x, y);
        for ny in cy.saturating_sub(1)..=(cy + 1).min(self.rows - 1) {
            for nx in cx.saturating_sub(1)..=(cx + 1).min(self.cols - 1) {
                for &i in self.cell(nx, ny) { f(i); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_close_pair_is_visited_exactly_once() {
        let pts: Vec<(f64, f64)> = (0..300)
            .map(|i| (((i * 37) % 500) as f64 + 0.3, ((i * 91) % 400) as f64 + 0.7))
            .collect();
        let cell = 40.0;
        let mut grid = Grid::default();
        grid.rebuild(cell, 500.0, 400.0, pts.iter().copied());
        let mut seen = std::collections::HashSet::new();
        grid.for_each_pair(|i, j| assert!(seen.insert((i.min(j), i.max(j))), "pair visited twice"));
        for i in 0..pts.len() {
            for j in i + 1..pts.len() {
                let (dx, dy) = (pts[i].0 - pts[j].0, pts[i].1 - pts[j].1);
                if dx * dx + dy * dy < cell * cell {
                    assert!(seen.contains(&(i, j)), "close pair ({i},{j}) missed");
                }
            }
        }
    }
}
