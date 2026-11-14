//! Scenes of Sue Writer's document view: the pages, composition mode,
//! the Format menu and the comments.

use std::time::Duration;

use super::super::Tour;
use super::writer_view;

/// Scene 14. Sue Writer on a sample novel, which this scene makes and the
/// later Sue Writer scenes use.
pub(super) fn open(tour: &mut Tour<'_>) -> Result<(), String> {
    let novel = tour
        .runtime
        .with_context(
            &mut tour.camera.painter,
            tour.camera.now,
            sue_writer::sample_project,
        )
        .ok_or("the sample novel was not made")?;
    tour.novel = Some(novel);
    tour.open_novel()?;
    // Let the fonts the novel uses load, then settle.
    for _ in 0..3 {
        tour.settle();
        std::thread::sleep(Duration::from_millis(400));
    }
    tour.shoot("14-sue-writer")
}

/// Scene 17. Composition mode.
pub(super) fn composition(tour: &mut Tour<'_>) -> Result<(), String> {
    writer_view(tour, "view/composition", "17-sue-writer-composition")
}

/// Scene 19. The Format menu.
pub(super) fn format_menu(tour: &mut Tour<'_>) -> Result<(), String> {
    writer_view(tour, "menu/Format", "19-sue-writer-format-menu")
}

/// 19a. The comments beside the text (Review, Comments), shown and put
/// away again.
pub(super) fn comments(tour: &mut Tour<'_>) -> Result<(), String> {
    let writer = tour.writer.clone();
    tour.command(&writer, "review.comments");
    tour.shoot("19a-sue-writer-comments")?;
    tour.command(&writer, "review.comments");
    Ok(())
}
