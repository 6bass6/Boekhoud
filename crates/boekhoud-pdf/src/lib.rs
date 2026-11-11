//! Invoice PDF rendering with the PDF standard fonts (Helvetica), so no font files are needed.
//!
//! Text is encoded as WinAnsi (covers Dutch accents and the euro sign). The caller supplies all
//! strings already formatted; this crate only lays them out.

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

#[derive(Debug, Clone, Default)]
pub struct Party {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PdfLine {
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub vat: String,
    pub amount: String,
}

#[derive(Debug, Clone, Default)]
pub struct InvoicePdf {
    /// "FACTUUR" or "CREDITNOTA"
    pub title: String,
    pub seller: Party,
    pub buyer: Party,
    /// Label/value pairs shown at the top right (number, dates, reference).
    pub meta: Vec<(String, String)>,
    pub lines: Vec<PdfLine>,
    /// Label/value pairs below the lines; the last one is printed bold.
    pub totals: Vec<(String, String)>,
    /// Mandatory notices such as "Btw verlegd" or the KOR statement.
    pub notices: Vec<String>,
    pub payment_text: Option<String>,
    pub notes: Option<String>,
    /// Footer line (legal name, KvK, btw-id, IBAN).
    pub footer: String,
    pub document_title: String,
}

const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 50.0;
const REGULAR: Name = Name(b"F1");
const BOLD: Name = Name(b"F2");

/// Helvetica / Helvetica-Bold glyph widths (1/1000 em) for ASCII 32..=126.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833,
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556,
    556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334,
    260, 334, 584,
];
const HELV_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833,
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611,
    556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389,
    280, 389, 584,
];

/// Map a character to its WinAnsi byte; unsupported characters become '?'.
fn winansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        '\u{a0}'..='\u{ff}' => c as u32 as u8,
        '€' => 0x80,
        '‚' => 0x82,
        '„' => 0x84,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        _ => b'?',
    }
}

fn encode(s: &str) -> Vec<u8> {
    s.chars().map(winansi).collect()
}

/// Approximate base letter for width lookup of accented Latin-1 characters.
fn width_char(c: char) -> char {
    match c {
        'à'..='å' => 'a',
        'ç' => 'c',
        'è'..='ë' => 'e',
        'ì'..='ï' => 'i',
        'ñ' => 'n',
        'ò'..='ö' => 'o',
        'ù'..='ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'À'..='Å' => 'A',
        'Ç' => 'C',
        'È'..='Ë' => 'E',
        'Ì'..='Ï' => 'I',
        'Ñ' => 'N',
        'Ò'..='Ö' => 'O',
        'Ù'..='Ü' => 'U',
        '€' => '0',
        '–' => '-',
        '—' => 'M',
        '‘' | '’' | '‚' => '\'',
        '“' | '”' | '„' => '"',
        _ => c,
    }
}

pub fn text_width(s: &str, size: f32, bold: bool) -> f32 {
    let table = if bold { &HELV_BOLD } else { &HELV };
    let units: u32 = s
        .chars()
        .map(|c| {
            let c = width_char(c);
            if (' '..='~').contains(&c) {
                table[c as usize - 32] as u32
            } else {
                556
            }
        })
        .sum();
    units as f32 * size / 1000.0
}

/// Greedy word wrap to `max_width` points.
pub fn wrap(s: &str, size: f32, bold: bool, max_width: f32) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in s.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if text_width(&candidate, size, bold) <= max_width || line.is_empty() {
                line = candidate;
            } else {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        out.push(line);
    }
    out
}

struct Writer {
    pages: Vec<Content>,
    y: f32,
}

impl Writer {
    fn content(&mut self) -> &mut Content {
        self.pages.last_mut().expect("at least one page")
    }

    fn text(&mut self, x: f32, y: f32, s: &str, size: f32, bold: bool) {
        let c = self.content();
        c.begin_text();
        c.set_font(if bold { BOLD } else { REGULAR }, size);
        c.next_line(x, y);
        c.show(Str(&encode(s)));
        c.end_text();
    }

    fn text_right(&mut self, right: f32, y: f32, s: &str, size: f32, bold: bool) {
        let w = text_width(s, size, bold);
        self.text(right - w, y, s, size, bold);
    }

    fn rule(&mut self, y: f32, gray: f32) {
        let c = self.content();
        c.set_stroke_rgb(gray, gray, gray);
        c.set_line_width(0.6);
        c.move_to(MARGIN, y);
        c.line_to(PAGE_W - MARGIN, y);
        c.stroke();
    }

    fn new_page(&mut self) {
        self.pages.push(Content::new());
        self.y = PAGE_H - MARGIN;
    }

    /// Start a new page if fewer than `needed` points remain above the footer.
    fn ensure(&mut self, needed: f32, inv: &InvoicePdf) {
        if self.y - needed < MARGIN + 40.0 {
            self.new_page();
            self.text(
                MARGIN,
                self.y,
                &format!("{} (vervolg)", inv.title.to_lowercase()),
                9.0,
                false,
            );
            self.y -= 24.0;
            table_header(self);
        }
    }
}

const COL_QTY: f32 = 360.0;
const COL_PRICE: f32 = 435.0;
const COL_VAT: f32 = 480.0;
const COL_AMOUNT: f32 = PAGE_W - MARGIN;
const DESC_WIDTH: f32 = 270.0;

fn table_header(w: &mut Writer) {
    let y = w.y;
    w.text(MARGIN, y, "Omschrijving", 9.0, true);
    w.text_right(COL_QTY, y, "Aantal", 9.0, true);
    w.text_right(COL_PRICE, y, "Prijs", 9.0, true);
    w.text_right(COL_VAT, y, "Btw", 9.0, true);
    w.text_right(COL_AMOUNT, y, "Bedrag", 9.0, true);
    w.rule(y - 5.0, 0.6);
    w.y -= 20.0;
}

/// Render the invoice to PDF bytes.
pub fn render_invoice(inv: &InvoicePdf) -> Vec<u8> {
    let mut w = Writer {
        pages: vec![Content::new()],
        y: PAGE_H - MARGIN,
    };

    // Seller (top left) and title (top right).
    let top = w.y;
    w.text(MARGIN, top - 4.0, &inv.seller.name, 15.0, true);
    let mut y = top - 22.0;
    for l in &inv.seller.lines {
        w.text(MARGIN, y, l, 9.0, false);
        y -= 12.0;
    }
    w.text_right(PAGE_W - MARGIN, top - 4.0, &inv.title, 20.0, true);

    // Buyer (left) and meta (right).
    let block = top - 120.0;
    w.text(MARGIN, block, &inv.buyer.name, 10.0, true);
    let mut by = block - 13.0;
    for l in &inv.buyer.lines {
        w.text(MARGIN, by, l, 10.0, false);
        by -= 13.0;
    }
    let mut my = block;
    for (label, value) in &inv.meta {
        w.text(340.0, my, label, 9.0, false);
        w.text_right(PAGE_W - MARGIN, my, value, 9.0, true);
        my -= 13.0;
    }
    w.y = by.min(my) - 30.0;

    table_header(&mut w);
    for line in &inv.lines {
        let desc = wrap(&line.description, 9.5, false, DESC_WIDTH);
        w.ensure(13.0 * desc.len() as f32 + 6.0, inv);
        let y = w.y;
        w.text_right(COL_QTY, y, &line.quantity, 9.5, false);
        w.text_right(COL_PRICE, y, &line.unit_price, 9.5, false);
        w.text_right(COL_VAT, y, &line.vat, 9.5, false);
        w.text_right(COL_AMOUNT, y, &line.amount, 9.5, false);
        for (i, d) in desc.iter().enumerate() {
            w.text(MARGIN, y - 13.0 * i as f32, d, 9.5, false);
        }
        w.y -= 13.0 * desc.len() as f32 + 6.0;
    }
    w.rule(w.y + 8.0, 0.6);
    w.y -= 8.0;

    // Totals.
    w.ensure(16.0 * inv.totals.len() as f32 + 10.0, inv);
    let n = inv.totals.len();
    for (i, (label, value)) in inv.totals.iter().enumerate() {
        let bold = i + 1 == n;
        let size = if bold { 11.0 } else { 9.5 };
        if bold {
            w.rule(w.y + 10.0, 0.75);
            w.y -= 4.0;
        }
        w.text_right(COL_VAT + 5.0 - 60.0, w.y, label, size, bold);
        w.text_right(COL_AMOUNT, w.y, value, size, bold);
        w.y -= 16.0;
    }
    w.y -= 14.0;

    // Notices, payment instruction, notes.
    let paragraphs: Vec<(String, bool)> = inv
        .notices
        .iter()
        .map(|n| (n.clone(), true))
        .chain(inv.payment_text.iter().map(|p| (p.clone(), false)))
        .chain(inv.notes.iter().map(|n| (n.clone(), false)))
        .collect();
    for (p, bold) in paragraphs {
        let lines = wrap(&p, 9.5, bold, PAGE_W - 2.0 * MARGIN);
        w.ensure(13.0 * lines.len() as f32 + 8.0, inv);
        for l in lines {
            w.text(MARGIN, w.y, &l, 9.5, bold);
            w.y -= 13.0;
        }
        w.y -= 8.0;
    }

    // Footer and page numbers on every page.
    let total = w.pages.len();
    for (i, page) in w.pages.iter_mut().enumerate() {
        let footer = format!("{}    —    pagina {} van {}", inv.footer, i + 1, total);
        let fw = text_width(&footer, 7.5, false);
        page.set_stroke_rgb(0.75, 0.75, 0.75);
        page.set_line_width(0.5);
        page.move_to(MARGIN, MARGIN + 12.0);
        page.line_to(PAGE_W - MARGIN, MARGIN + 12.0);
        page.stroke();
        page.set_fill_rgb(0.35, 0.35, 0.35);
        page.begin_text();
        page.set_font(REGULAR, 7.5);
        page.next_line(((PAGE_W - fw) / 2.0).max(MARGIN), MARGIN);
        page.show(Str(&encode(&footer)));
        page.end_text();
    }

    // Assemble the document.
    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let font = Ref::new(3);
    let font_bold = Ref::new(4);
    let info = Ref::new(5);
    let first_page = 10;
    let page_ids: Vec<Ref> = (0..total).map(|i| Ref::new(first_page + 2 * i as i32)).collect();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids(page_ids.iter().copied()).count(total as i32);
    pdf.type1_font(font)
        .base_font(Name(b"Helvetica"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    pdf.type1_font(font_bold)
        .base_font(Name(b"Helvetica-Bold"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    {
        let mut di = pdf.document_info(info);
        di.title(TextStr(&inv.document_title));
        di.producer(TextStr("Boekhoud"));
        di.finish();
    }
    for (i, content) in w.pages.into_iter().enumerate() {
        let page_id = page_ids[i];
        let content_id = Ref::new(page_id.get() + 1);
        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, PAGE_W, PAGE_H));
        page.parent(tree);
        page.contents(content_id);
        {
            let mut res = page.resources();
            let mut fonts = res.fonts();
            fonts.pair(REGULAR, font);
            fonts.pair(BOLD, font_bold);
        }
        page.finish();
        pdf.stream(content_id, &content.finish());
    }
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(lines: usize) -> InvoicePdf {
        InvoicePdf {
            title: "FACTUUR".into(),
            seller: Party { name: "Voorbeeld Apps".into(), lines: vec!["Voorbeeldstraat 1".into(), "1234 AB Utrecht".into()] },
            buyer: Party { name: "Stichting Voorbeeld".into(), lines: vec!["Laan 2".into(), "5678 CD Zwolle".into()] },
            meta: vec![("Factuurnummer".into(), "2026-0001".into()), ("Factuurdatum".into(), "11-11-2026".into())],
            lines: (0..lines)
                .map(|i| PdfLine {
                    description: format!("Lezing over app-ontwikkeling, café-editie nr. {i} met een lange omschrijving die over meerdere regels loopt"),
                    quantity: "1".into(),
                    unit_price: "€ 1.000,00".into(),
                    vat: "21%".into(),
                    amount: "€ 1.000,00".into(),
                })
                .collect(),
            totals: vec![("Subtotaal".into(), "€ 1.000,00".into()), ("Btw 21%".into(), "€ 210,00".into()), ("Totaal".into(), "€ 1.210,00".into())],
            notices: vec![],
            payment_text: Some("Graag betalen vóór 11-12-2026.".into()),
            notes: None,
            footer: "Voorbeeld Apps · KvK 12345678".into(),
            document_title: "Factuur 2026-0001".into(),
        }
    }

    #[test]
    fn renders_valid_pdf_with_euro_sign() {
        let bytes = render_invoice(&sample(2));
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(15).any(|w| w == b"WinAnsiEncoding"));
        // The euro sign is encoded as WinAnsi 0x80 inside the content.
        assert!(bytes.contains(&0x80));
    }

    #[test]
    fn long_invoices_paginate() {
        let bytes = render_invoice(&sample(60));
        let pages = bytes.windows(10).filter(|w| w == b"/Type /Pag").count();
        assert!(pages >= 3, "expected several pages, got {pages}");
    }

    #[test]
    fn widths_and_wrapping() {
        assert_eq!(
            text_width("1.000,00", 10.0, false),
            (5.0 * 556.0 + 2.0 * 278.0 + 556.0) / 100.0
        );
        let lines = wrap("een twee drie vier vijf zes zeven acht negen tien", 10.0, false, 60.0);
        assert!(lines.len() > 1);
        assert!(lines
            .iter()
            .all(|l| text_width(l, 10.0, false) <= 60.0 || !l.contains(' ')));
        assert_eq!(encode("€é"), vec![0x80, 0xe9]);
    }
}
