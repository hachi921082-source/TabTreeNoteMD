//! GitHub issue #262: moves or copies a memo's image folder between two tab folders during a
//! cross-tab memo move/copy (`docs/plans/tab-lock/CRASH-RECOVERY.md` §10). Callers build both
//! paths through the #263 guard (`fs::path_safety`); this module never overwrites or merges into
//! an existing destination and never follows a symlink/junction.

use std::path::{Path, PathBuf};

use crate::fs::path_safety::is_link_or_reparse_point;
use crate::fs::retry::with_retry;
use crate::fs::self_write_registry::SelfWriteRegistry;

/// One planned folder transfer (`src` in the source tab, `dst` in the target tab).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderTransfer {
    pub src: PathBuf,
    pub dst: PathBuf,
}

/// `src` の中身を既存の `dst` へ再帰コピーする(`dst` は自分で作成済みのフォルダであること)。
/// サブフォルダは `dst` 内に新規作成してから中身をコピーする。シンボリックリンク/ジャンクションは
/// 辿らずスキップする(タブ外の内容を取り込まないため)。
fn copy_dir_contents(src: &Path, dst: &Path, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let meta = std::fs::symlink_metadata(entry.path())?;
        if is_link_or_reparse_point(&meta) {
            tracing::warn!(path = %entry.path().display(), "image folder copy: link skipped");
            continue;
        }
        let to = dst.join(entry.file_name());
        registry.register(&to);
        if meta.is_dir() {
            std::fs::create_dir(&to)?;
            copy_dir_contents(&entry.path(), &to, registry)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// `dst` を新規作成して `src` を再帰コピーする。`dst` の作成に失敗(既存など)したら何も消さずに
/// 失敗を返す。存在チェック後に他プロセスが `dst` を作った場合でも、自分が作っていないフォルダを
/// 後始末で削除しないため。作成後のコピー途中で失敗したときだけ、作りかけの `dst` を消す。
fn create_and_copy_dir(src: &Path, dst: &Path, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    registry.register(dst);
    std::fs::create_dir(dst)?;
    if let Err(err) = copy_dir_contents(src, dst, registry) {
        registry.register(dst);
        let _ = std::fs::remove_dir_all(dst);
        return Err(err);
    }
    Ok(())
}

/// `src` を `dst` へ移動(`copy == false`)またはコピーする。`dst` に何か(ファイル/フォルダ/リンク)が
/// あれば失敗。移動はリネーム(再試行付き)、失敗時は別ボリューム想定でコピー + 元フォルダ削除へ
/// フォールバックする(`.md` の移動と同じ方針)。コピー完了後の元フォルダ削除失敗はログのみ
/// (移動先は完全なので成功扱い)。コピー途中の失敗では作りかけの `dst` を消してから失敗を返す
/// (自分で作成した `dst` に限る。`create_and_copy_dir` 参照)。
pub fn transfer_folder(src: &Path, dst: &Path, copy: bool, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    if std::fs::symlink_metadata(dst).is_ok() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "image folder destination already exists"));
    }
    if !copy {
        registry.register(src);
        registry.register(dst);
        if with_retry(|| std::fs::rename(src, dst)).is_ok() {
            return Ok(());
        }
    }
    create_and_copy_dir(src, dst, registry)?;
    if !copy {
        registry.register(src);
        if let Err(err) = with_retry(|| std::fs::remove_dir_all(src)) {
            tracing::warn!(error = %err, path = %src.display(), "image folder move: copied, but the source folder could not be fully removed");
        }
    }
    Ok(())
}

/// 完了済みの転送を逆順に取り消す(移動は戻し、コピーは移動先を削除)。ベストエフォートで、
/// 取り消せなかった件があれば `false`。
pub fn undo_transfers(done: &[FolderTransfer], copy: bool, registry: &SelfWriteRegistry) -> bool {
    let mut ok = true;
    for t in done.iter().rev() {
        let result = if copy {
            registry.register(&t.dst);
            with_retry(|| std::fs::remove_dir_all(&t.dst))
        } else {
            transfer_folder(&t.dst, &t.src, false, registry)
        };
        if let Err(err) = result {
            tracing::error!(error = %err, path = %t.dst.display(), "image folder transfer: failed to undo");
            ok = false;
        }
    }
    ok
}

/// すべての転送を順に行う。1件でも失敗したら、それまでの転送を取り消して `Err` を返す
/// (失敗時に何も変わらない状態へ戻す。取り消し自体の失敗はログのみ)。
pub fn transfer_all(transfers: &[FolderTransfer], copy: bool, registry: &SelfWriteRegistry) -> std::io::Result<()> {
    for (i, t) in transfers.iter().enumerate() {
        if let Err(err) = transfer_folder(&t.src, &t.dst, copy, registry) {
            tracing::error!(error = %err, path = %t.src.display(), "image folder transfer failed; undoing earlier transfers");
            undo_transfers(&transfers[..i], copy, registry);
            return Err(err);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ttn-folder-transfer-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_folder(dir: &Path) {
        std::fs::create_dir_all(dir.join("サブ")).unwrap();
        std::fs::write(dir.join("001.png"), b"a").unwrap();
        std::fs::write(dir.join("サブ").join("画像 2.png"), b"b").unwrap();
    }

    fn assert_folder(dir: &Path) {
        assert_eq!(std::fs::read(dir.join("001.png")).unwrap(), b"a");
        assert_eq!(std::fs::read(dir.join("サブ").join("画像 2.png")).unwrap(), b"b");
    }

    #[test]
    fn move_and_copy_japanese_folder() {
        let root = tempdir();
        let registry = SelfWriteRegistry::new();
        let src = root.join("元タブ").join("日記　その1");
        make_folder(&src);
        std::fs::create_dir_all(root.join("先タブ")).unwrap();
        let copied = root.join("先タブ").join("日記　その1");
        transfer_folder(&src, &copied, true, &registry).unwrap();
        assert_folder(&src);
        assert_folder(&copied);
        let moved = root.join("先タブ").join("別名");
        transfer_folder(&src, &moved, false, &registry).unwrap();
        assert!(!src.exists());
        assert_folder(&moved);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn existing_destination_is_never_merged() {
        let root = tempdir();
        let registry = SelfWriteRegistry::new();
        let src = root.join("a");
        make_folder(&src);
        let dst = root.join("b");
        std::fs::create_dir_all(&dst).unwrap();
        for copy in [false, true] {
            assert_eq!(transfer_folder(&src, &dst, copy, &registry).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        }
        assert_folder(&src);
        assert_eq!(std::fs::read_dir(&dst).unwrap().count(), 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn create_and_copy_never_removes_existing_destination() {
        // 存在チェック後に他プロセスが dst を作った競合を、既存 dst への直接呼び出しで再現する
        let root = tempdir();
        let registry = SelfWriteRegistry::new();
        let src = root.join("元");
        make_folder(&src);
        let dst = root.join("先");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("他人の.png"), b"x").unwrap();
        assert!(create_and_copy_dir(&src, &dst, &registry).is_err());
        assert_eq!(std::fs::read(dst.join("他人の.png")).unwrap(), b"x");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn transfer_all_undoes_earlier_transfers_on_failure() {
        let root = tempdir();
        let registry = SelfWriteRegistry::new();
        let (a, b) = (root.join("甲"), root.join("乙"));
        make_folder(&a);
        make_folder(&b);
        let target = root.join("先");
        std::fs::create_dir_all(target.join("乙")).unwrap(); // 2件目の移動先を塞ぐ
        let transfers = vec![
            FolderTransfer { src: a.clone(), dst: target.join("甲") },
            FolderTransfer { src: b.clone(), dst: target.join("乙") },
        ];
        for copy in [false, true] {
            assert!(transfer_all(&transfers, copy, &registry).is_err());
            assert_folder(&a);
            assert_folder(&b);
            assert!(!target.join("甲").exists());
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
