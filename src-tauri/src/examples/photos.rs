//! Example: photos from your Apple Photos library (PhotoKit).
//!
//! Picks a random Favorite (or a recent photo if you have no Favorites),
//! asks Photos for its image (downloading from iCloud if needed), and shrinks
//! it to a JPEG with `sips`. Photos never opens. macOS asks once for access.
//! Shrunk copies are cached in `<data dir>/photo-cache` (newest 60 kept).

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Granted,
    Denied,
    Unknown,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    /// Stable id for this photo (hash of Photos' identifier).
    pub id: String,
    /// Photos' own identifier, for favoriting.
    pub asset: String,
    /// Marked as a Favorite in Photos.
    pub favorite: bool,
    /// ISO date the photo was taken, if known.
    pub taken: Option<String>,
    pub width: u64,
    pub height: u64,
    /// `data:image/jpeg;base64,…`, so it can't be loaded by anything without the app token.
    pub image: String,
}

/// Longest edge of the cached JPEG, px. The Edge is 720px tall.
const MAX_EDGE: u32 = 1600;
const CACHE_KEEP: usize = 60;

fn short_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(s.as_bytes());
    d.iter().take(10).map(|b| format!("{b:02x}")).collect()
}

/// Shrink image bytes (HEIC, JPEG, PNG…) to a JPEG with macOS's `sips`.
fn shrink(data: &[u8], out: &Path) -> Result<(), String> {
    let tmp = out.with_extension("src");
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg", "-s", "formatOptions", "80", "-Z", &MAX_EDGE.to_string()])
        .arg(&tmp)
        .arg("--out")
        .arg(out)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&tmp);
    match status? {
        s if s.success() => Ok(()),
        s => Err(format!("sips failed ({s})")),
    }
}

/// Drop the oldest cached files beyond `keep`.
fn prune(dir: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, p) in files.into_iter().skip(keep) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use base64::Engine;
    use block2::RcBlock;
    use objc2::AnyThread;
    use objc2_foundation::{NSData, NSDictionary, NSString};
    use objc2_photos::{
        PHAccessLevel, PHAsset, PHAssetMediaType, PHAuthorizationStatus, PHFetchOptions, PHImageManager, PHImageRequestOptions,
        PHImageRequestOptionsVersion, PHPhotoLibrary,
    };
    use std::sync::mpsc;
    use std::time::Duration;

    pub fn access() -> Access {
        let s = unsafe { PHPhotoLibrary::authorizationStatusForAccessLevel(PHAccessLevel::ReadWrite) };
        if s == PHAuthorizationStatus::Authorized || s == PHAuthorizationStatus::Limited {
            Access::Granted
        } else if s == PHAuthorizationStatus::NotDetermined {
            Access::Unknown
        } else {
            Access::Denied
        }
    }

    /// Show the macOS Photos permission prompt (once). Waits up to 2 minutes.
    pub fn request_access() -> Access {
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |_s: PHAuthorizationStatus| {
            let _ = tx.send(());
        });
        unsafe { PHPhotoLibrary::requestAuthorizationForAccessLevel_handler(PHAccessLevel::ReadWrite, &block) };
        let _ = rx.recv_timeout(Duration::from_secs(120));
        access()
    }

    fn fetch(favorites: bool) -> objc2::rc::Retained<objc2_photos::PHFetchResult<PHAsset>> {
        unsafe {
            let opts = PHFetchOptions::init(PHFetchOptions::alloc());
            if favorites {
                let p = objc2_foundation::NSPredicate::predicateWithFormat_argumentArray(&NSString::from_str("favorite == YES"), None);
                opts.setPredicate(Some(&p));
            } else {
                opts.setFetchLimit(500);
                let sort = objc2_foundation::NSSortDescriptor::sortDescriptorWithKey_ascending(Some(&NSString::from_str("creationDate")), false);
                opts.setSortDescriptors(Some(&objc2_foundation::NSArray::from_retained_slice(&[sort])));
            }
            PHAsset::fetchAssetsWithMediaType_options(PHAssetMediaType::Image, Some(&opts))
        }
    }

    /// Mark or unmark a photo as a Favorite in Photos.
    pub fn set_favorite(asset_id: &str, favorite: bool) -> Result<(), String> {
        if access() != Access::Granted {
            return Err("Photos access isn't allowed".into());
        }
        let ids = objc2_foundation::NSArray::from_retained_slice(&[NSString::from_str(asset_id)]);
        let found = unsafe { PHAsset::fetchAssetsWithLocalIdentifiers_options(&ids, None) };
        if unsafe { found.count() } == 0 {
            return Err("That photo is no longer in your library".into());
        }
        let asset = unsafe { found.objectAtIndex(0) };
        let block = RcBlock::new(move || unsafe {
            objc2_photos::PHAssetChangeRequest::changeRequestForAsset(&asset).setFavorite(favorite);
        });
        unsafe { PHPhotoLibrary::sharedPhotoLibrary().performChangesAndWait_error(&*block as *const _ as *mut _) }
            .map_err(|e| e.localizedDescription().to_string())
    }

    pub fn random(cache_dir: &Path) -> Result<Option<Photo>, String> {
        if access() != Access::Granted {
            return Ok(None);
        }
        let mut result = fetch(true);
        if unsafe { result.count() } == 0 {
            result = fetch(false);
        }
        let n = unsafe { result.count() };
        if n == 0 {
            return Ok(None);
        }
        let idx = (uuid::Uuid::new_v4().as_u128() % n as u128) as usize;
        let asset = unsafe { result.objectAtIndex(idx) };
        let ident = unsafe { asset.localIdentifier() }.to_string();
        let id = short_hash(&ident);
        let taken = unsafe { asset.creationDate() }
            .map(|d| d.timeIntervalSince1970())
            .and_then(|t| chrono::DateTime::from_timestamp(t as i64, 0))
            .map(|d| d.with_timezone(&chrono::Local).to_rfc3339());
        let (w, h) = unsafe { (asset.pixelWidth() as u64, asset.pixelHeight() as u64) };
        let favorite = unsafe { asset.isFavorite() };

        std::fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
        let file = cache_dir.join(format!("{id}.jpg"));
        if !file.exists() {
            let (tx, rx) = mpsc::channel::<Option<Vec<u8>>>();
            let block = RcBlock::new(move |data: *mut NSData, _uti: *mut NSString, _o: objc2_image_io::CGImagePropertyOrientation, _i: *mut NSDictionary| {
                let bytes = unsafe { data.as_ref() }.map(|d| d.to_vec());
                let _ = tx.send(bytes);
            });
            unsafe {
                let opts = PHImageRequestOptions::init(PHImageRequestOptions::alloc());
                opts.setNetworkAccessAllowed(true); // iCloud "Optimize storage" keeps originals in the cloud
                opts.setSynchronous(true);
                opts.setVersion(PHImageRequestOptionsVersion::Current); // with your edits
                PHImageManager::defaultManager().requestImageDataAndOrientationForAsset_options_resultHandler(&asset, Some(&opts), &block);
            }
            let data = rx.recv_timeout(Duration::from_secs(60)).ok().flatten().ok_or("Photos didn't return the image")?;
            shrink(&data, &file)?;
            prune(cache_dir, CACHE_KEEP);
        }
        let jpeg = std::fs::read(&file).map_err(|e| e.to_string())?;
        Ok(Some(Photo {
            id,
            asset: ident,
            favorite,
            taken,
            width: w,
            height: h,
            image: format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpeg)),
        }))
    }
}

pub fn access() -> Access {
    #[cfg(target_os = "macos")]
    return mac::access();
    #[cfg(not(target_os = "macos"))]
    Access::Denied
}

pub fn request_access() -> Access {
    #[cfg(target_os = "macos")]
    return mac::request_access();
    #[cfg(not(target_os = "macos"))]
    Access::Denied
}

/// Photos identifiers look like "8A1C…-…/L0/001".
pub fn valid_asset_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 200 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/')
}

pub fn set_favorite(asset_id: &str, favorite: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::set_favorite(asset_id, favorite);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (asset_id, favorite);
        Err("Photos needs macOS".into())
    }
}

/// A random photo, or None without access or with an empty library.
pub fn random(cache_root: &Path) -> Result<Option<Photo>, String> {
    #[cfg(target_os = "macos")]
    return mac::random(&cache_root.join("photo-cache"));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cache_root;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_asset_ids() {
        assert!(valid_asset_id("8A1C2B3D-1234-ABCD-9999-0123456789AB/L0/001"));
        assert!(!valid_asset_id("../x") && !valid_asset_id("") && !valid_asset_id("a b"));
    }

    #[test]
    fn hash_is_stable_and_short() {
        assert_eq!(short_hash("ABC/L0/001"), short_hash("ABC/L0/001"));
        assert_eq!(short_hash("x").len(), 20);
    }

    #[test]
    fn prune_keeps_newest() {
        let dir = std::env::temp_dir().join(format!("edge-prune-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            std::fs::write(dir.join(format!("{i}.jpg")), b"x").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        prune(&dir, 2);
        let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, vec!["3.jpg", "4.jpg"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
