//! Local text recognition with the OCR engine built into Windows (`Windows.Media.Ocr`).
//! PDFs are rendered page by page with `Windows.Data.Pdf`; nothing leaves the PC.

/// Recognise the text of an image (PNG, JPEG, …) or PDF. Lines are separated by newlines.
#[cfg(windows)]
pub fn recognise(bytes: &[u8], mime_type: &str) -> Result<String, String> {
    win::recognise(bytes, mime_type).map_err(|e| format!("Tekstherkenning mislukt: {}", e.message()))
}

#[cfg(not(windows))]
pub fn recognise(_bytes: &[u8], _mime_type: &str) -> Result<String, String> {
    Err("Tekstherkenning werkt alleen op Windows.".into())
}

#[cfg(windows)]
mod win {
    use windows::core::{Result, HSTRING};
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    /// Maximum number of PDF pages read (invoice data is on the first pages).
    const MAX_PAGES: u32 = 3;
    /// Render width for PDF pages: about 240 dpi for A4, sharp enough for small print.
    const PDF_WIDTH: u32 = 2000;

    fn stream_from(bytes: &[u8]) -> Result<InMemoryRandomAccessStream> {
        let stream = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(bytes)?;
        writer.StoreAsync()?.join()?;
        writer.FlushAsync()?.join()?;
        writer.DetachStream()?;
        stream.Seek(0)?;
        Ok(stream)
    }

    fn bitmap_from(stream: &InMemoryRandomAccessStream) -> Result<SoftwareBitmap> {
        let decoder = BitmapDecoder::CreateAsync(stream)?.join()?;
        let bitmap = decoder.GetSoftwareBitmapAsync()?.join()?;
        SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8)
    }

    fn engine() -> Result<OcrEngine> {
        // Prefer Dutch when its OCR language pack is installed, else the user's languages.
        for tag in ["nl-NL", "nl"] {
            if let Ok(lang) = Language::CreateLanguage(&HSTRING::from(tag)) {
                if OcrEngine::IsLanguageSupported(&lang).unwrap_or(false) {
                    return OcrEngine::TryCreateFromLanguage(&lang);
                }
            }
        }
        OcrEngine::TryCreateFromUserProfileLanguages()
    }

    fn ocr(engine: &OcrEngine, bitmap: &SoftwareBitmap) -> Result<String> {
        let max = OcrEngine::MaxImageDimension()?;
        if bitmap.PixelWidth()? as u32 > max || bitmap.PixelHeight()? as u32 > max {
            // Too large for the engine: skip this image rather than fail the whole document.
            return Ok(String::new());
        }
        let result = engine.RecognizeAsync(bitmap)?.join()?;
        let lines = result.Lines()?;
        let mut out = String::new();
        for i in 0..lines.Size()? {
            out.push_str(&lines.GetAt(i)?.Text()?.to_string_lossy());
            out.push('\n');
        }
        Ok(out)
    }

    pub fn recognise(bytes: &[u8], mime_type: &str) -> Result<String> {
        let engine = engine()?;
        let stream = stream_from(bytes)?;
        if mime_type != "application/pdf" {
            return ocr(&engine, &bitmap_from(&stream)?);
        }
        let doc = PdfDocument::LoadFromStreamAsync(&stream)?.join()?;
        let width = PDF_WIDTH.min(OcrEngine::MaxImageDimension()?);
        let mut text = String::new();
        for i in 0..doc.PageCount()?.min(MAX_PAGES) {
            let page = doc.GetPage(i)?;
            let options = PdfPageRenderOptions::new()?;
            options.SetDestinationWidth(width)?;
            let out = InMemoryRandomAccessStream::new()?;
            page.RenderWithOptionsToStreamAsync(&out, &options)?.join()?;
            out.Seek(0)?;
            text.push_str(&ocr(&engine, &bitmap_from(&out)?)?);
        }
        Ok(text)
    }
}
