// 5.1: Cross-package library call test — wc calls cat's library fn directly
use std::io::BufRead;

fn empty_input() -> Box<dyn BufRead> {
    Box::new(std::io::empty())
}

#[test]
fn wc_and_cat_library_calls() {
    // Create a temp file
    let dir = std::env::temp_dir().join("zr_cross_pkg_test");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("test.txt");
    std::fs::write(&file, "hello world\nfoo bar baz\n").unwrap();

    // Call cat as a library — no process spawn
    let records: Vec<_> = zr_cat::cat(
        Some(file.clone()),
        None,
        None,
        empty_input(),
    )
    .unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].content, "hello world");
    assert_eq!(records[1].content, "foo bar baz");

    // Call wc as a library — no process spawn
    let result = zr_wc::wc(
        Some(file.clone()),
        empty_input(),
    )
    .unwrap();
    assert_eq!(result.lines, 2);
    assert_eq!(result.words, 5);

    // Cleanup
    std::fs::remove_file(&file).ok();
    std::fs::remove_dir(&dir).ok();
}
