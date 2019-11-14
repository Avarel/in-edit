use std::io;
use std::cell::{RefCell, Cell};
use crate::{MultilineTerm, Cursor};

pub trait Renderer {
    fn draw(&self, term: &MultilineTerm) -> io::Result<()>;
    fn redraw(&self, term: &MultilineTerm) -> io::Result<()>;
    fn clear_draw(&self, term: &MultilineTerm) -> io::Result<()>;
}

#[derive(Default)]
pub struct FullRenderer {
    #[doc(hidden)]
    pds: Cell<PreviousDrawState>,
    /// Function to draw the prompt.
    gutter: Option<Box<dyn Fn(usize, &MultilineTerm) -> String>>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviousDrawState {
    pub height: usize,
    pub cursor: Cursor 
}

impl Renderer for FullRenderer {
    /// Draw the prompt.
    fn draw(&self, term: &MultilineTerm) -> io::Result<()> {
        // Handle empty buffer.
        if term.buffers.is_empty() {
            if let Some(f) = &self.gutter {
                term.inner.write_str(&f(0, term))?;
            }
            self.update_pds(|pds| pds.height = 1);
            return Ok(())
        }

        // Print out the contents.
        for i in 0..term.buffers.len() {
            self.draw_line(term, i)?;
            if i < term.buffers.len() - 1 {
                // The last line should not have any new-line attached to it.
                self.new_line(term)?;
            }
        }

        self.update_pds(|pds| {
            pds.height = term.buffers.len();
            pds.cursor.line = term.buffers.len() - 1;
            pds.cursor.index = term.buffers.last().unwrap().len();
        });

        self.draw_cursor(term)
    }

    /// Clear the drawn prompt on the screen.
    fn clear_draw(&self, term: &MultilineTerm) -> io::Result<()> {
        self.move_cursor_to_bottom(term)?;
        term.inner.clear_line()?;
        term.inner.clear_last_lines(self.pds().height - 1)?;

        self.update_pds(|pds| {
            pds.height = 0;
            pds.cursor.line = 0;
            pds.cursor.index = 0;
        });

        Ok(())
    }

    /// Redraw the screen.
    fn redraw(&self, term: &MultilineTerm) -> io::Result<()> {
        self.clear_draw(term)?;
        self.draw(term)
    }
}

impl FullRenderer {
    pub fn with_gutter<F: 'static + Fn(usize, &MultilineTerm) -> String>(f: F) -> Self {
        FullRenderer {
            pds: Cell::new(PreviousDrawState::default()),
            gutter: Some(Box::new(f))
        }
    }

    #[doc(hidden)]
    fn update_pds<F: FnOnce(&mut PreviousDrawState)>(&self, f: F) {
        let mut pds = self.pds();
        f(&mut pds);
        self.pds.set(pds);
    }

    #[doc(hidden)]
    fn pds(&self) -> PreviousDrawState {
        self.pds.get()
    }

    // Position the cursor.
    // At this point the cursor is pointed at the very end of the last line.
    pub fn draw_cursor(&self, term: &MultilineTerm) -> io::Result<()> {
        self.move_cursor_to_line(term, term.cursor.line)?;
        self.move_cursor_to_index(term, term.cursor.index.min(term.current_line_len()))
    }

    /// Draw the line given an index.
    /// This method does not move the cursor.
    pub fn draw_line(&self, term: &MultilineTerm, line: usize) -> io::Result<()> {
        if let Some(f) = &self.gutter {
            term.inner.write_str(&f(line, term))?;
        }
        term.inner.write_str(&term.buffers[line])
    }

    /// Insert a new line on the screen.
    #[inline]
    pub fn new_line(&self, term: &MultilineTerm) -> io::Result<()> {
        term.inner.write_line("")
    }

    /// Move the current cursor to the last line.
    #[inline]
    pub fn move_cursor_to_bottom(&self, term: &MultilineTerm) -> io::Result<()> {
        self.move_cursor_down(term, self.pds().height - self.pds().cursor.line - 1)
    }

    pub fn move_cursor_to_line(&self, term: &MultilineTerm, line: usize) -> io::Result<()> {
        let pds_line = self.pds().cursor.line;

        if pds_line > line {
            self.move_cursor_up(term, pds_line - line)
        } else if pds_line < line {
            self.move_cursor_down(term, line - pds_line)
        } else {
            Ok(())
        }
    }

    pub fn move_cursor_to_index(&self, term: &MultilineTerm, index: usize) -> io::Result<()> {
        let pds_index = self.pds().cursor.index;

        if index < pds_index {
            self.move_cursor_left(term, pds_index - index)
        } else if index > pds_index {
            self.move_cursor_right(term, index - pds_index)
        } else {
            Ok(())
        }
    }

    /// Move the cursor to the end of the current line.
    /// This method is not safe to use if the cursor is not at `line:index`,
    #[inline]
    pub fn move_cursor_to_end(&self, term: &MultilineTerm) -> io::Result<()> {
        let pds = self.pds();
        let len = term.current_line_len();
        if pds.cursor.index > len {
            self.move_cursor_left(term, pds.cursor.index - len)
        } else if pds.cursor.index < len {
            self.move_cursor_right(term, len - pds.cursor.index)
        } else {
            Ok(())
        }
    }

    /// Move the cursor to the beginning of the line.
    #[inline]
    pub fn move_cursor_to_start(&self, term: &MultilineTerm) -> io::Result<()> {
        self.move_cursor_left(term, term.cursor.index)?;
        Ok(())
    }

    /// Move the cursor one line up.
    #[inline]
    pub fn move_cursor_up(&self, term: &MultilineTerm, n: usize) -> io::Result<()> {
        term.inner.move_cursor_up(n)?;
        self.update_pds(|pds| pds.cursor.line -= n);
        Ok(())
    }

    /// Move the cursor one line down.
    #[inline]
    pub fn move_cursor_down(&self, term: &MultilineTerm, n: usize) -> io::Result<()> {
        term.inner.move_cursor_down(n)?;
        self.update_pds(|pds| pds.cursor.line += n);
        Ok(())
    }

    /// Move the cursor leftward using nondestructive backspaces.
    #[inline]
    pub fn move_cursor_left(&self, term: &MultilineTerm, n: usize) -> io::Result<()> {
        term.inner.move_cursor_left(n)?;
        self.update_pds(|pds| pds.cursor.index -= n);
        Ok(())
    }

    /// Move the cursor rightward.
    #[inline]
    pub fn move_cursor_right(&self, term: &MultilineTerm, n: usize) -> io::Result<()> {
        term.inner.move_cursor_right(n)?;
        self.update_pds(|pds| pds.cursor.index += n);
        Ok(())
    }
}

#[derive(Default)]
pub struct LazyRenderer {
    /// The lazy renderer wraps around a full renderer, using its methods when necessary.
    inner: FullRenderer,
    #[doc(hidden)]
    pbuf: RefCell<Vec<String>>
}

impl Renderer for LazyRenderer {
    fn draw(&self, term: &MultilineTerm) -> io::Result<()> {
        self.inner.draw(term)?;
        self.pbuf.replace(term.buffers().clone());
        Ok(())
    }

    fn redraw(&self, term: &MultilineTerm) -> io::Result<()> {
        match self.find_diff(term) {
            Diff::NoChange => Ok(()),
            Diff::RedrawCursor => self.inner.draw_cursor(term),
            Diff::RedrawLine(line) => self.redraw_line(term, line),
            Diff::RedrawAll => {
                self.clear_draw(term)?;
                self.draw(term)
            }
        }
    }

    fn clear_draw(&self, term: &MultilineTerm) -> io::Result<()> {
        self.pbuf.borrow_mut().clear();
        self.inner.clear_draw(term)
    }
}

impl LazyRenderer {
    pub fn wrap(renderer: FullRenderer) -> Self {
        Self {
            inner: renderer,
            pbuf: RefCell::new(Vec::new()),
        }
    }

    fn find_diff(&self, term: &MultilineTerm) -> Diff {
        let old = self.pbuf.borrow();
        let new = term.buffers();
        
        if old.len() != new.len() {
            return Diff::RedrawAll
        }
        
        let mut changes = 0;
        let mut line = 0;

        for i in 0..old.len() {
            if old[i] != new[i] {
                changes += 1;
                line = i;
            }
        }

        match changes {
            0 if self.inner.pds().cursor != *term.cursor() => Diff::RedrawCursor,
            0 => Diff::NoChange,
            1 => Diff::RedrawLine(line),
            _ => Diff::RedrawAll
        }
    }

    fn redraw_line(&self, term: &MultilineTerm, line: usize) -> io::Result<()> {
        self.inner.move_cursor_to_line(term, line)?;
        term.inner.clear_line()?;
        self.inner.draw_line(term, line)?;

        let buf = term.buffers()[line].clone();
        self.inner.update_pds(|pds| pds.cursor.index = buf.len());
        self.pbuf.borrow_mut()[line] = buf;

        self.inner.draw_cursor(term)
    }
}

#[derive(Debug)]
enum Diff {
    NoChange,
    RedrawCursor,
    RedrawLine(usize),
    RedrawAll
}