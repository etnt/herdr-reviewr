//! Switching the reviewed worktree: the pane restarts on the pick as a pane opened there would.

mod common;

use common::{FakeTarget, Repo, app_on};
use herdr_reviewr::app::{App, FooterAction, Mode, Tab};
use herdr_reviewr::config::Config;
use herdr_reviewr::git::canonical;
use herdr_reviewr::keymap::default_keymap;
use herdr_reviewr::model::{Comment, Rev, Scope, Side};
use herdr_reviewr::{handle_key, handle_mouse, switched_app};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 120, 40);

fn paths(app: &App) -> Vec<String> {
    app.entries.iter().map(|e| e.path.clone()).collect()
}

fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::from(code), AREA, default_keymap()).unwrap();
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn comment() -> Comment {
    Comment {
        file: "main.txt".into(),
        side: Side::New,
        start: 1,
        end: 1,
        lines: "+edited".into(),
        text: "unsent".into(),
        diff_anchored: true,
        rev: Rev::Worktree,
    }
}

/// A repo on `main` with two linked worktrees, `zeta` and `company-pages`.
fn clone_with_worktrees() -> (Repo, common::LinkedWorktree, common::LinkedWorktree) {
    let repo = Repo::init();
    repo.write("main.txt", "base\n");
    repo.commit_all("base");
    let zeta = repo.add_worktree("zeta");
    let pages = repo.add_worktree("company-pages");
    (repo, zeta, pages)
}

/// Where the pane will restart, as the OS spells it (macOS temp dirs sit under `/private`).
fn switch_target(app: &App) -> Option<std::path::PathBuf> {
    app.switch_to.as_ref().map(|choice| canonical(&choice.path))
}

/// The pane restarted on `path`, loaded as the run loop loads it.
fn switch_to(old: &mut App, path: &std::path::Path, name: &str) -> App {
    let cfg = Config::parse([path.to_string_lossy().into_owned()]);
    let mut switched = switched_app(&cfg, old, name);
    switched.reload().unwrap();
    switched
}

fn row_names(app: &App) -> Vec<String> {
    let wp = app.worktree_picker.as_ref().expect("the picker is open");
    wp.visible().iter().map(|r| r.name.clone()).collect()
}

#[test]
fn the_picker_lists_the_current_first_then_by_branch_and_filters() {
    let (repo, _zeta, pages) = clone_with_worktrees();
    let mut app = app_on(&repo);
    press(&mut app, KeyCode::Char('W'));
    assert_eq!(app.mode, Mode::WorktreePick);
    let current = herdr_reviewr::git::checked_out_branch(repo.path()).unwrap().unwrap();
    assert_eq!(row_names(&app), [current.as_str(), "company-pages", "zeta"]);
    let wp = app.worktree_picker.as_ref().unwrap();
    assert!(wp.rows[0].current && wp.cursor == 0, "the cursor starts on the current worktree");

    // Enter on the current worktree changes nothing.
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.mode.clone(), app.switch_to.clone()), (Mode::Normal, None));

    press(&mut app, KeyCode::Char('W'));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    type_text(&mut app, "comp");
    assert_eq!(row_names(&app), ["company-pages"]);
    let wp = app.worktree_picker.as_ref().unwrap();
    assert_eq!(wp.cursor, 0, "a filter edit puts the highlight on the best match");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        switch_target(&app),
        Some(canonical(pages.path())),
        "no comments: the pick switches"
    );
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn unsent_comments_get_the_switch_question() {
    let (repo, _zeta, pages) = clone_with_worktrees();
    let mut app = app_on(&repo);
    app.store.add(comment());
    press(&mut app, KeyCode::Char('W'));
    type_text(&mut app, "comp");
    press(&mut app, KeyCode::Enter);
    assert!(app.switch_question().is_some(), "enter asks first");
    assert_eq!(app.switch_to, None);
    let bands: Vec<FooterAction> = app.footer_bands().into_iter().map(|(a, _)| a).collect();
    assert_eq!(
        bands,
        [FooterAction::SwitchDiscard, FooterAction::Cancel, FooterAction::Send, FooterAction::Copy]
    );

    // Any other key returns to the picker, the filter kept.
    press(&mut app, KeyCode::Char('x'));
    assert!(app.switch_question().is_none());
    assert_eq!(app.mode, Mode::WorktreePick);
    assert_eq!(row_names(&app), ["company-pages"], "the stray key never reached the filter");

    // Esc, a paste, or a click answers it the same way.
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    assert!(app.switch_question().is_none());
    assert_eq!(app.mode, Mode::WorktreePick, "esc leaves the question, not the picker");
    press(&mut app, KeyCode::Enter);
    app.input_paste("zeta");
    assert!(app.switch_question().is_none());
    assert_eq!(row_names(&app), ["company-pages"], "the paste never reached the filter");
    press(&mut app, KeyCode::Enter);
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    handle_mouse(&mut app, click, AREA, &[], default_keymap(), &FakeTarget::ok()).unwrap();
    assert!(app.switch_question().is_none());
    assert_eq!((app.mode.clone(), app.switch_to.clone()), (Mode::WorktreePick, None));

    // The bound switch key answers by switching and dropping the comments.
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('W'));
    assert_eq!(switch_target(&app), Some(canonical(pages.path())));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn send_from_the_switch_question_stays() {
    let (repo, _zeta, _pages) = clone_with_worktrees();
    let mut app = app_on(&repo);
    app.store.add(comment());
    press(&mut app, KeyCode::Char('W'));
    type_text(&mut app, "comp");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.switch_to, None, "a send never switches");
    assert!(app.worktree_picker.is_none());
    assert_eq!(app.store.len(), 1, "no agent took them: the comments stay");
}

#[test]
fn a_worktree_removed_after_listing_is_gone_not_switched_to() {
    let (repo, _zeta, pages) = clone_with_worktrees();
    let mut app = app_on(&repo);
    press(&mut app, KeyCode::Char('W'));
    type_text(&mut app, "comp");
    std::fs::remove_dir_all(pages.path()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.switch_to, None);
    assert!(app.status.starts_with("worktree gone"), "{}", app.status);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn w_is_silent_and_unadvertised_without_another_worktree() {
    let repo = Repo::init();
    repo.write("main.txt", "base\n");
    repo.commit_all("base");
    let mut app = app_on(&repo);
    let advertised =
        |app: &App| app.footer_bands().iter().any(|(a, _)| *a == FooterAction::WorktreePick);
    assert!(!advertised(&app));
    press(&mut app, KeyCode::Char('W'));
    assert_eq!(app.mode, Mode::Normal, "nothing to switch to: the key stays silent");

    // A new worktree shows the key once the watcher reports it (`tests/watch.rs`).
    let wt = repo.add_worktree("other");
    assert!(!advertised(&app), "listed once, at the first reload");
    app.relist_worktrees();
    assert!(advertised(&app));

    // One deleted but never pruned touches nothing the watcher sees; `W` finds nothing to list
    // and the footer stops advertising it.
    std::fs::remove_dir_all(wt.path()).unwrap();
    assert!(advertised(&app));
    press(&mut app, KeyCode::Char('W'));
    assert_eq!(app.mode, Mode::Normal, "a prunable worktree is nothing to switch to");
    assert!(!advertised(&app), "the fresh listing corrected the footer");
}

#[test]
fn a_switch_reviews_the_pick_and_keeps_only_the_view() {
    let repo = Repo::init();
    repo.write("main.txt", "base\n");
    repo.commit_all("base");
    let wt = repo.add_worktree("company-pages");
    std::fs::write(wt.path().join("page.txt"), "new\n").unwrap();
    repo.write("main.txt", "edited\n");

    let mut old = app_on(&repo);
    old.wrap = false;
    old.set_scope(Scope::Branch).unwrap();
    old.store.add(Comment { text: "stays behind".into(), ..comment() });
    let opened_in = old.opened_in.clone();
    // Where herdr last saw this pane, which a move changes after launch.
    old.herdr.ids.pane = Some("w9:p3".into());
    old.herdr.ids.workspace = Some("w9".into());

    let switched = switch_to(&mut old, wt.path(), "company-pages");

    let mut fresh = herdr_reviewr::app::App::new(switched.repo.clone(), Scope::Branch, None);
    fresh.reload().unwrap();
    assert_eq!(paths(&switched), paths(&fresh), "the pick, as a pane opened there sees it");
    assert_eq!(paths(&switched), ["page.txt"]);
    assert_eq!(switched.store.len(), 0, "comments stay with the worktree they were made on");
    assert_eq!(switched.scope, Scope::Branch, "the scope carries");
    assert!(!switched.wrap, "wrap carries");
    assert_eq!(switched.opened_in, opened_in, "a switch never moves where the pane opened");
    assert_eq!(
        switched.herdr.ids.pane.as_deref(),
        Some("w9:p3"),
        "the live pane id, not the env's"
    );
    assert_eq!(switched.herdr.ids.workspace.as_deref(), Some("w9"));
    assert_eq!(switched.status, "reviewing company-pages until this pane closes");
}

#[test]
fn a_copy_names_the_worktree_once_the_pane_switched() {
    let repo = Repo::init();
    repo.write("main.txt", "base\n");
    repo.commit_all("base");
    let wt = repo.add_worktree("company-pages");
    std::fs::write(wt.path().join("main.txt"), "edited\n").unwrap();
    repo.write("main.txt", "edited\n");

    let mut old = app_on(&repo);
    old.store.add(comment());
    let copied = FakeTarget::ok();
    assert!(old.export(&copied));
    assert_eq!(copied.last(), "main.txt:1\n+edited\nunsent", "where it opened: today's review");

    let mut switched = switch_to(&mut old, wt.path(), "company-pages");
    switched.store.add(comment());
    assert!(switched.export(&copied));
    let expected = format!("Worktree: {}\n\nmain.txt:1\n+edited\nunsent", switched.repo.display());
    assert_eq!(copied.last(), expected);

    // Back where it opened, the review needs no worktree line again.
    let mut back = switch_to(&mut switched, repo.path(), "main");
    back.store.add(comment());
    assert!(back.export(&copied));
    assert_eq!(copied.last(), "main.txt:1\n+edited\nunsent");
}

#[test]
fn every_view_of_a_switched_pane_matches_a_pane_opened_there() {
    let repo = Repo::init();
    repo.write("main.txt", "base\n");
    repo.commit_all("base");
    let wt = repo.add_worktree("company-pages");
    std::fs::write(wt.path().join("page.txt"), "committed\n").unwrap();
    let in_wt = wt.path().to_str().unwrap();
    repo.git(&["-C", in_wt, "add", "-A"]);
    repo.git(&["-C", in_wt, "commit", "-qm", "page"]);
    std::fs::write(wt.path().join("draft.txt"), "uncommitted\n").unwrap();

    for scope in [Scope::Uncommitted, Scope::Branch] {
        let mut old = app_on(&repo);
        old.set_scope(scope).unwrap();
        let mut switched = switch_to(&mut old, wt.path(), "company-pages");
        let mut fresh = App::new(switched.repo.clone(), scope, None);
        fresh.reload().unwrap();
        assert_eq!(paths(&switched), paths(&fresh), "{scope:?}");
        common::enter_tab(&mut switched, Tab::AllFiles);
        common::enter_tab(&mut fresh, Tab::AllFiles);
        assert_eq!(paths(&switched), paths(&fresh), "All files from {scope:?}");
        assert!(paths(&switched).contains(&"page.txt".to_string()));
    }
}

#[test]
fn a_commit_pick_scope_stays_behind() {
    let repo = Repo::init();
    repo.write("a.txt", "a\n");
    repo.commit_all("a");
    let wt = repo.add_worktree("other");
    let mut old = app_on(&repo);
    old.scope = Scope::Commits;
    let switched = switch_to(&mut old, wt.path(), "other");
    assert_eq!(switched.scope, Scope::Uncommitted, "the default scope, not the old pick's");
}
