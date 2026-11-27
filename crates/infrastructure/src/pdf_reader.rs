//! Lecture des PDF avec `pdf-extract` : le texte de chaque page, mot par mot, avec sa position.

use institution::{PdfReader, PositionedWord};
use pdf_extract::{Document, MediaBox, OutputDev, OutputError, Transform, output_doc};

pub struct PdfExtractReader;

/// Un caractère et sa place.
struct Glyph {
    x: f64,
    y: f64,
    width: f64,
    size: f64,
    text: String,
}

/// Collecte les caractères de chaque page, dans l'ordre où le PDF les dessine.
#[derive(Default)]
struct Collector {
    pages: Vec<Vec<Glyph>>,
}

impl OutputDev for Collector {
    fn begin_page(
        &mut self,
        _page_num: u32,
        _media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.pages.push(Vec::new());
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        _spacing: f64,
        _font_size: f64,
        char: &str,
    ) -> Result<(), OutputError> {
        let size = (trm.m11 * trm.m11 + trm.m12 * trm.m12).sqrt();
        if let Some(page) = self.pages.last_mut() {
            page.push(Glyph {
                x: trm.m31,
                y: trm.m32,
                width: width * size,
                size,
                text: char.to_owned(),
            });
        }
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}

/// Deux caractères d'une même ligne sont dans le même mot s'ils se touchent (à une fraction de la taille près).
fn words_of(mut glyphs: Vec<Glyph>) -> Vec<PositionedWord> {
    // Ligne par ligne (à 1,5 point près), puis de gauche à droite.
    glyphs.sort_by(|a, b| b.y.total_cmp(&a.y));
    let mut rows: Vec<Vec<Glyph>> = Vec::new();
    for glyph in glyphs {
        match rows
            .last_mut()
            .filter(|row| (row[0].y - glyph.y).abs() < 1.5)
        {
            Some(row) => row.push(glyph),
            None => rows.push(vec![glyph]),
        }
    }
    let mut words = Vec::new();
    for mut row in rows {
        row.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut current: Option<PositionedWord> = None;
        for glyph in row {
            if glyph.text.trim().is_empty() {
                words.extend(current.take());
                continue;
            }
            let gap_limit = (glyph.size * 0.25).max(1.0);
            match current.as_mut() {
                Some(word) if glyph.x - word.x1 <= gap_limit => {
                    word.text.push_str(&glyph.text);
                    word.x1 = glyph.x + glyph.width;
                }
                _ => {
                    words.extend(current.take());
                    current = Some(PositionedWord {
                        text: glyph.text.clone(),
                        x0: glyph.x,
                        x1: glyph.x + glyph.width,
                        y: glyph.y,
                    });
                }
            }
        }
        words.extend(current);
    }
    words
}

impl PdfReader for PdfExtractReader {
    fn read(&self, bytes: &[u8]) -> Result<Vec<Vec<PositionedWord>>, String> {
        let unreadable = || "Ce PDF est illisible : il est peut-être abîmé.".to_owned();
        let mut doc = Document::load_mem(bytes).map_err(|_| unreadable())?;
        if doc.is_encrypted() {
            doc.decrypt("").map_err(|_| {
                "Ce PDF est protégé par un mot de passe : télécharge-le sans protection.".to_owned()
            })?;
        }
        let mut collector = Collector::default();
        output_doc(&doc, &mut collector).map_err(|_| unreadable())?;
        let pages: Vec<Vec<PositionedWord>> = collector.pages.into_iter().map(words_of).collect();
        if pages.iter().all(|p| p.is_empty()) {
            return Err(
                "Ce PDF ne contient pas de texte (une image scannée ?) : je ne sais lire que les relevés dont le texte est sélectionnable."
                    .into(),
            );
        }
        Ok(pages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuse_ce_qui_n_est_pas_un_pdf() {
        assert!(PdfExtractReader.read(b"pas un pdf").is_err());
    }

    /// Lit un vrai relevé quand `WORTHFOLD_TEST_PDF` désigne un fichier : ignoré sinon (les relevés sont privés).
    #[test]
    fn lit_un_vrai_releve_si_on_en_donne_un() {
        let Ok(path) = std::env::var("WORTHFOLD_TEST_PDF") else {
            return;
        };
        let pages = PdfExtractReader
            .read(&std::fs::read(path).unwrap())
            .unwrap();
        assert!(!pages.is_empty());
    }
}
