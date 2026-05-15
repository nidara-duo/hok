use once_cell::unsync::OnceCell;
use scoop_hash::ChecksumBuilder;
use std::io::Read;
use tracing::{debug, warn};

use crate::{
    env, error::Fallible, internal, persist, psmodule, shim, shortcut, Error, Event, QueryOption,
    Session,
};

use super::{
    download::{self, DownloadSize},
    query, resolve, Package,
};

/// Options that may be used to tweak behavior of package sync operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum SyncOption {
    /// Assume YES on all prompts.
    ///
    /// # Note
    ///
    /// This option will also suppress the prompt for package candidate selection.
    /// A built-in candidate selection algorithm will be used to select the
    /// proper candidate. This may not be the desired behavior in some cases.
    ///
    /// Enabling this option will also suppress the calculation of download size.
    AssumeYes,

    /// Download package only.
    ///
    /// # Note
    ///
    /// To sync packages by just downloading and caching them without installing
    /// or upgrading, this option can be used. Transcation will be stopped after
    /// the download is done.
    DownloadOnly,

    /// Force operations on held packages.
    ///
    /// # Note
    ///
    /// Held packages are ignored during the replace, upgrade or uninstall
    /// operations by default. The option can be used to escape the hold and
    /// enforce operations on the held packages.
    ///
    /// Packages will be held again after the replace or upgrade operation.
    EscapeHold,

    /// Ignore local cache and force package download.
    ///
    /// # Note
    ///
    /// This option is not intended to be used with the [`Offline`][1]
    /// option.
    ///
    /// [1]: SyncOption::Offline
    IgnoreCache,

    /// Ignore transaction failure.
    ///
    /// The sync operation processes packages in the transaction one by one
    /// according to the dependency order. By default, the transaction will be
    /// aborted if any failure occurs during the operation.
    ///
    /// # Note
    ///
    /// This option can be used to ignore the failure and continue the operation
    /// to commit the remaining packages in the transaction.
    ///
    /// When a failure occurs, the operation will be stopped immediately and
    /// a rollback will be performed on the exact package causing the failure
    /// while successfully committed packages will be kept be as they are. The
    /// rest of the unpocessed packages will be skipped, and the error will be
    /// returned.
    ///
    /// **NO rollback will be performed if this option is enabled**, which means
    /// there may be broken packages being committed to the system.
    IgnoreFailure,

    /// Do not install dependencies.
    ///
    /// # Note
    ///
    /// By default, dependencies of the pending installation package will be
    /// resolved and installed **recursively** if they are not installed yet.
    /// One can opt in this option to disable the default behavior. However,
    /// it is not recommended to do so since it clearly breaks the dependency
    /// relationship, and may stop the dependents from working properly.
    NoDependencies,

    /// Stop checking hash of downloaded packages.
    ///
    /// # Note
    ///
    /// Integrity check helps to ensure the downloaded packages are not corrupted
    /// or tampered. Hash check will be performed by default. In some cases, user
    /// may want to skip the check to force the installation or upgrade of the
    /// packages. By opting in this option, the hash check will be skipped.
    ///
    /// It is highly **NOT** recommended to use this option unless you really
    /// know what you are doing.
    NoHashCheck,

    /// Do not upgrade packages.
    ///
    /// This option is not intended to be used with the [`OnlyUpgrade`][1] option.
    ///
    /// [1]: SyncOption::OnlyUpgrade
    NoUpgrade,

    /// Do not replace packages.
    ///
    /// # Note
    ///
    /// When a package is installed and a same-named package is proposed to be
    /// installed, a replace operation will be performed if the proposed package
    /// is from a different bucket from the installed one.
    ///
    /// By opting in this option, the replace operation will be suppressed.
    NoReplace,

    /// Offline mode.
    ///
    /// # Note
    ///
    /// This option is useful when user wants to install or upgrade packages
    /// with existing local cached packages. By opting in this option and having
    /// valid caches prepared, network access can be avoided to perform the sync
    /// operation. However, the transaction may fail if there is any package file
    /// missing or invalid cache.
    ///
    /// This option is basically the opposite of the [`IgnoreCache`][1] option.
    ///
    /// [1]: SyncOption::IgnoreCache
    Offline,

    /// Upgrade packages only.
    ///
    /// Use this option to specify a sync operation of only upgrading packages.
    ///
    /// This option is not intended to be used with the [`NoUpgrade`][1] option.
    ///
    /// [1]: SyncOption::NoUpgrade
    OnlyUpgrade,

    /// Uninstall packages.
    ///
    /// Use this option to specify a sync operation of only uninstalling packages.
    Remove,

    /// Purge uninstall.
    ///
    /// # Note
    ///
    /// By enabling this option, persistent data of the pending removal packages
    /// will be removed simultaneously.
    ///
    /// This option only takes effect with the [`Remove`][1] option.
    ///
    /// [1]: SyncOption::Remove
    Purge,

    /// Cascade uninstall.
    ///
    /// # Note
    ///
    /// By opt in this option, dependencies of the pending removal package
    /// will also be removed **recursively** if they are not required by other
    /// installed packages.
    ///
    /// This option only takes effect with the [`Remove`][1] option.
    ///
    /// [1]: SyncOption::Remove
    Cascade,

    /// Disable dependent check.
    ///
    /// # Note
    ///
    /// By default, a reverse dependencies check will be performed on the pending
    /// removal package. If any installed package depends on the pending removal
    /// package, the removal operation will be aborted.
    ///
    /// The default behavior can be modified by opting in this option, however,
    /// it is not recommended to do so since it clearly breaks the dependency
    /// relationship, and may stop the dependents from working properly.
    ///
    /// This option only takes effect with the [`Remove`][1] option.
    ///
    /// [1]: SyncOption::Remove
    NoDependentCheck,
}

/// Transaction of sync operation.
///
/// # Note
///
/// A transaction is a set of packages that will be installed, upgraded, replaced
/// or removed. The transaction is calculated by the sync operation and can be
/// used to prompt the user to confirm the operation.
#[derive(Clone)]
pub struct Transaction {
    /// Packages that will be installed with the transaction.
    install: OnceCell<Vec<Package>>,

    /// Packages that will be upgraded with the transaction.
    upgrade: OnceCell<Vec<Package>>,

    /// Packages that will be replaced with the transaction.
    replace: OnceCell<Vec<Package>>,

    /// Packages that will be removed with the transaction.
    remove: OnceCell<Vec<Package>>,

    /// Total download size of the transaction.
    download_size: OnceCell<DownloadSize>,
}

impl Transaction {
    fn new() -> Transaction {
        Transaction {
            install: OnceCell::new(),
            upgrade: OnceCell::new(),
            replace: OnceCell::new(),
            remove: OnceCell::new(),
            download_size: OnceCell::new(),
        }
    }

    fn set_install(&self, packages: Vec<Package>) {
        let _ = self.install.set(packages);
    }

    fn set_upgrade(&self, packages: Vec<Package>) {
        let _ = self.upgrade.set(packages);
    }

    fn set_replace(&self, packages: Vec<Package>) {
        let _ = self.replace.set(packages);
    }

    fn set_remove(&self, packages: Vec<Package>) {
        let _ = self.remove.set(packages);
    }

    fn set_download_size(&self, download_size: DownloadSize) -> bool {
        self.download_size.set(download_size).is_ok()
    }

    fn add_view(&self) -> Vec<&Package> {
        self.install_view()
            .into_iter()
            .chain(self.upgrade_view())
            .chain(self.replace_view())
            .flatten()
            .collect::<Vec<_>>()
    }

    /// Get packages that will be installed with the transaction.
    ///
    /// # Returns
    ///
    /// A reference to the vector of packages that will be installed or `None`
    /// if no packages will be installed.
    pub fn install_view(&self) -> Option<&Vec<Package>> {
        self.install.get()
    }

    /// Get packages that will be upgraded with the transaction.
    ///
    /// # Returns
    ///
    /// A reference to the vector of packages that will be upgraded or `None`
    /// if no packages will be upgraded.
    pub fn upgrade_view(&self) -> Option<&Vec<Package>> {
        self.upgrade.get()
    }

    /// Get packages that will be replaced with the transaction.
    ///
    /// # Returns
    ///
    /// A reference to the vector of packages that will be replaced or `None`
    /// if no packages will be replaced.
    pub fn replace_view(&self) -> Option<&Vec<Package>> {
        self.replace.get()
    }

    /// Get packages that will be removed with the transaction.
    ///
    /// # Returns
    ///
    /// A reference to the vector of packages that will be removed or `None`
    /// if no packages will be removed.
    pub fn remove_view(&self) -> Option<&Vec<Package>> {
        self.remove.get()
    }

    /// Get the total download size of the transaction.
    ///
    /// # Returns
    ///
    /// A `DownloadSize` reference that contains the total download size of the
    /// transaction.
    pub fn download_size(&self) -> Option<&DownloadSize> {
        self.download_size.get()
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Self::new()
    }
}

/// Sync operation: install and/or upgrade packages.
pub fn install(session: &Session, queries: &[&str], options: &[SyncOption]) -> Fallible<()> {
    debug!(
        "Entering sync::install, queries: {:?}, options: {:?}",
        queries, options
    );
    let mut packages = vec![];

    let only_upgrade = options.contains(&SyncOption::OnlyUpgrade);
    let escape_hold = options.contains(&SyncOption::EscapeHold);

    if only_upgrade {
        if queries == vec!["*"] {
            // If there are no queries (upgrade everything), we leave the old logic
            packages = query::query_installed(session, &["*"], &[QueryOption::Upgradable])?;
            packages = packages
                .into_iter()
                .map(|p| p.upgradable().cloned().unwrap())
                .collect::<Vec<_>>();
        } else {
            let synced = query::query_synced(session, &["*"], &[])?;
            for &query in queries {
                let (query_bucket, query_name) = query.split_once('/').unwrap_or(("", query));

                // 1. Check if the package exists in the manifests at all
                let exists = synced.iter().any(|p| {
                    let bucket_matched = query_bucket.is_empty() || p.bucket() == query_bucket;
                    let name_matched = p.name() == query_name;
                    bucket_matched && name_matched
                });

                if !exists {
                    if let Some(tx) = session.emitter() {
                        let _ = tx.send(Event::PackageNotFound(query.to_owned()));
                    }
                    return Err(Error::PackageNotFound(query.to_owned()));
                }

                // 2. Check if it is installed in the system
                let installed = query::query_installed(session, &[query], &[])?;
                if installed.is_empty() {
                    if let Some(tx) = session.emitter() {
                        let _ = tx.send(Event::PackageNotFound(query.to_owned()));
                    }
                    return Err(Error::PackageNotFound(query.to_owned()));
                }

                // 3. If installed, check for updates
                let mut upgradable =
                    query::query_installed(session, &[query], &[QueryOption::Upgradable])?;
                if let Some(p) = upgradable.pop() {
                    packages.push(p.upgradable().cloned().unwrap());
                }
            }
        }
    } else {
        let synced = query::query_synced(session, &["*"], &[])?;

        for &query in queries {
            let mut matched = synced
                .iter()
                .filter(|&p| {
                    let (query_bucket, query_name) = query.split_once('/').unwrap_or(("", query));
                    let bucket_matched = query_bucket.is_empty() || p.bucket() == query_bucket;
                    let name_matched = p.name() == query_name;
                    bucket_matched && name_matched
                })
                .cloned()
                .collect::<Vec<_>>();

            match matched.len() {
                0 => {
                    if let Some(tx) = session.emitter() {
                        let _ = tx.send(Event::PackageNotFound(query.to_owned()));
                    }
                    return Err(Error::PackageNotFound(query.to_owned()));
                }
                1 => {
                    let p = matched.pop().unwrap();
                    if p.is_held() && !escape_hold {
                        continue;
                    }
                    if !packages.contains(&p) {
                        packages.push(p);
                    }
                }
                _ => {
                    let is_held = matched.iter().any(|p| p.is_held());
                    if is_held && !escape_hold {
                        continue;
                    }
                    resolve::select_candidate(session, &mut matched)?;
                    let p = matched.pop().unwrap();
                    if !packages.contains(&p) {
                        packages.push(p);
                    }
                }
            }
        }
    };

    if packages.is_empty() {
        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageNoOp);
        }
        return Ok(());
    }
    let transaction = Transaction::default();
    if !options.contains(&SyncOption::NoDependencies) {
        resolve::resolve_dependencies(session, &mut packages)?;
    }

    let (installed, installable): (Vec<_>, Vec<_>) =
        packages.into_iter().partition(|p| p.is_installed());
    let (upgradable, replaceable): (Vec<_>, Vec<_>) = installed
        .into_iter()
        .partition(|p| p.is_strictly_installed());

    if !only_upgrade && !installable.is_empty() {
        transaction.set_install(installable);
    }
    let upgradable = upgradable
        .into_iter()
        .filter(|p| p.upgradable_version().is_some())
        .collect::<Vec<_>>();
    if !options.contains(&SyncOption::NoUpgrade) && !upgradable.is_empty() {
        if !escape_hold {
            let (_held, upgradable): (Vec<_>, Vec<_>) =
                upgradable.into_iter().partition(|p| p.is_held());
            if !upgradable.is_empty() {
                transaction.set_upgrade(upgradable);
            }
        } else {
            transaction.set_upgrade(upgradable);
        }
    }
    if !options.contains(&SyncOption::NoReplace) && !replaceable.is_empty() {
        transaction.set_replace(replaceable);
    }

    let reuse_cache = !options.contains(&SyncOption::IgnoreCache);
    let packages = transaction.add_view();
    if packages.is_empty() {
        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageNoOp);
        }
        return Ok(());
    }

    debug!("Downloading packages...");
    let mut set =
        download::PackageSet::new(session, &packages, reuse_cache)?;
    if !options.contains(&SyncOption::Offline) {
        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageDownloadSizingStart);
        }
        let download_size = set.calculate_download_size()?;
        transaction.set_download_size(download_size);
    }

    if !options.contains(&SyncOption::AssumeYes) {
        if let Some(tx) = session.emitter() {
            if tx
                .send(Event::PromptTransactionNeedConfirm(transaction.clone()))
                .is_ok()
            {
                let rx = session.receiver().unwrap();
                let mut confirmed = false;
                while let Ok(event) = rx.recv() {
                    if let Event::PromptTransactionNeedConfirmResult(ret) = event {
                        confirmed = ret;
                        break;
                    }
                }
                if !confirmed {
                    return Ok(());
                }
            }
        }
    }

    if !options.contains(&SyncOption::Offline) {
        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageDownloadStart);
        }
        if let Err(e) = set.download() {
            debug!("Error during download: {}", e);
            return Err(e);
        }
        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageDownloadDone);
        }
    }

    if !options.contains(&SyncOption::NoHashCheck) {
        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageIntegrityCheckStart);
        }
        let config = session.config();
        let cache_root = config.cache_path();
        let mut buf = [0; 1024 * 64];

        for &pkg in packages.iter() {
            if pkg.version() == "nightly" {
                continue;
            }
            let files = pkg.download_filenames();
            let hashes = pkg.download_hashes();
            for (idx, (filename, hash)) in files.into_iter().zip(hashes).enumerate() {
                let path = cache_root.join(&filename);
                let mut hasher = ChecksumBuilder::new().algo(hash.algorithm())?.build();
                let mut file = std::fs::File::open(&path)?;
                loop {
                    let len = file.read(&mut buf)?;
                    if len == 0 {
                        break;
                    }
                    hasher.consume(&buf[..len]);
                }
                let actual = hasher.finalize();
                if actual != hash.value() {
                    return Err(Error::HashMismatch(super::HashMismatchContext::new(
                        pkg.name().to_owned(),
                        pkg.download_urls()[idx].to_owned(),
                        hash.value().to_owned(),
                        actual,
                    )));
                }
            }
        }
        if let Some(tx) = session.emitter() {
            let _ = tx.send(Event::PackageIntegrityCheckDone);
        }
    }

    debug!("Sync options: {:?}", options);
    debug!(
        "Should install: {}",
        !options.contains(&SyncOption::DownloadOnly)
    );
    debug!("Number of packages to install/upgrade: {}", packages.len());
    if !options.contains(&SyncOption::DownloadOnly) {
        for &pkg in packages.iter() {
            debug!("Starting installation for package: {}", pkg.name());
            let config = session.config();
            let app_dir = config.root_path().join("apps").join(pkg.name());
            let version_dir = app_dir.join(pkg.version());
            debug!(
                "App directory: {}, Version directory: {}",
                app_dir.display(),
                version_dir.display()
            );

            // 1. Clean and prepare install destination dir
            if version_dir.exists() {
                std::fs::remove_dir_all(&version_dir)?;
            }
            internal::fs::ensure_dir(&version_dir)?;

            // 2. Clean and prepare staging dir
            let staging_dir = app_dir.join(format!(".tmp-{}", pkg.version()));
            if staging_dir.exists() {
                let _ = std::fs::remove_dir_all(&staging_dir);
            }
            internal::fs::ensure_dir(&staging_dir)?;

            // 3. Prepare staging dir contents
            let filenames = pkg.download_filenames();
            let urls = pkg.download_urls();
            for (filename, url) in filenames.iter().zip(urls.iter()) {
                let src = config.cache_path().join(filename);
                if internal::archive::is_archive_url(url) {
                    internal::archive::extract(&src, &staging_dir)?;
                } else {
                    let real_filename = url
                        .split('/')
                        .next_back()
                        .unwrap_or(filename.as_str())
                        .split('?')
                        .next()
                        .unwrap_or(filename.as_str());
                    let dest = staging_dir.join(real_filename);
                    std::fs::copy(&src, &dest)?;
                }
            }

            // 4. Extract into version dir
            let extract_source = if let Some(extract_dirs) = pkg.manifest().extract_dir() {
                let mut p = staging_dir.clone();
                for d in extract_dirs {
                    p = p.join(d);
                }
                p
            } else {
                // Auto-detect: if archive extracted into a single subdirectory, use that
                let subdirs: Vec<_> = std::fs::read_dir(&staging_dir)?
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .collect();
                if subdirs.len() == 1 {
                    subdirs[0].path()
                } else {
                    staging_dir.clone()
                }
            };

            // 5. Move all contents into version_dir
            for entry in std::fs::read_dir(&extract_source)? {
                let entry = entry?;
                let dest = version_dir.join(entry.file_name());
                std::fs::rename(entry.path(), dest)?;
            }

            // 6. Clean up staging dir
            let _ = std::fs::remove_dir_all(&staging_dir);

            // 7. Create current junction
            let current_lnk = app_dir.join("current");
            if current_lnk.exists() || current_lnk.is_symlink() {
                let _ = internal::fs::remove_symlink(&current_lnk);
            }
            let output = std::process::Command::new("cmd")
                .args([
                    "/c",
                    "mklink",
                    "/J",
                    &current_lnk.to_string_lossy(),
                    &version_dir.to_string_lossy(),
                ])
                .output()?;
            if !output.status.success() {
                debug!(
                    "Junction creation failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            // 8. Create shims
            shim::add(session, pkg, &current_lnk)?;

            // 9. Write install.json
            let install_info = crate::package::InstallInfo::new(
                match std::env::consts::ARCH {
                    "x86_64" => "64bit".to_owned(),
                    "x86" => "32bit".to_owned(),
                    "aarch64" => "arm64".to_owned(),
                    other => other.to_owned(),
                },
                Some(pkg.bucket().to_owned()),
                Some(false),
                None,
            );
            let _ = internal::fs::write_json(version_dir.join("install.json"), install_info);

            // 10. Copy manifest.json
            let manifest_path = pkg.manifest().path();
            let dest_manifest = version_dir.join("manifest.json");
            debug!(
                "Manifest path: {}, exists: {}",
                manifest_path.display(),
                manifest_path.exists()
            );
            if manifest_path.exists() {
                match std::fs::copy(manifest_path, &dest_manifest) {
                    Ok(_) => debug!("Copied manifest to {}", dest_manifest.display()),
                    Err(e) => {
                        let msg = format!(
                            "Failed to copy manifest from {} to {}: {}",
                            manifest_path.display(),
                            dest_manifest.display(),
                            e
                        );
                        warn!("{}", msg);
                        return Err(crate::Error::Custom(msg));
                    }
                }
            } else {
                warn!(
                    "Manifest file does not exist at {}",
                    manifest_path.display()
                );
            }

            // 11. Clean up old version directory (upgrade only)
            if only_upgrade {
                if let Some(old_version) = pkg.installed_version() {
                    let old_version_dir = app_dir.join(old_version);
                    if old_version_dir.exists() && old_version_dir != version_dir {
                        let _ = std::fs::remove_dir_all(&old_version_dir);
                    }
                }
            }
        }
    }
    if let Some(tx) = session.emitter() {
        let _ = tx.send(Event::PackageSyncDone);
    }
    Ok(())
}

/// Sync operation: remove packages.
pub fn remove(session: &Session, queries: &[&str], options: &[SyncOption]) -> Fallible<()> {
    let mut packages = vec![];

    let installed = query::query_installed(session, &["*"], &[])?;
    let escape_hold = options.contains(&SyncOption::EscapeHold);

    for &name in queries {
        let mut matched = installed
            .iter()
            .filter(|&p| p.name() == name)
            .cloned()
            .collect::<Vec<_>>();

        if matched.is_empty() {
            return Err(Error::PackageNotFound(name.to_string()));
        }

        // It's impossible to have more than one installed packages for the same
        // package name.
        assert_eq!(matched.len(), 1);

        let pkg = matched.pop().unwrap();

        if pkg.is_held() && !escape_hold {
            continue;
        }

        packages.push(pkg);
    }

    let no_dependent_check = options.contains(&SyncOption::NoDependentCheck);
    if !no_dependent_check {
        let mut dependents = vec![];

        for pkg in packages.iter() {
            let mut result = installed
                .iter()
                .filter_map(|p| {
                    if packages.contains(p) {
                        return None;
                    }

                    let dep_names = p
                        .dependencies()
                        .into_iter()
                        .map(super::extract_name)
                        .collect::<Vec<_>>();

                    if dep_names.contains(&pkg.name().to_owned()) {
                        // p depends on pkg
                        Some((p.name().to_owned(), pkg.name().to_owned()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if result.is_empty() {
                continue;
            }

            dependents.append(&mut result);
        }

        if !dependents.is_empty() {
            return Err(Error::PackageDependentFound(dependents));
        }
    }

    let is_cascade = options.contains(&SyncOption::Cascade);
    if is_cascade {
        resolve::resolve_cascade(session, &mut packages, escape_hold)?;
    }

    if let Some(tx) = session.emitter() {
        let _ = tx.send(Event::PackageResolveDone);
    }

    let transaction = Transaction::default();

    // TODO: PowerShell hosting with execution context is not supported yet.
    // Perhaps at present we could call Scoop to do the removal for packages
    // using PS scripts...
    let (packages_with_script, _packages): (Vec<_>, Vec<_>) =
        packages.iter().partition(|p| p.has_uninstall_script());

    // TODO: support removal of packages with PowerShell script
    if !packages_with_script.is_empty() {
        let msg = format!("Found package(s) using PowerShell script:\n  {}\nRemoval of package with PowerShell script is not yet supported.",
        packages_with_script.iter().map(|p| p.name()).collect::<Vec<_>>().join("  "));
        return Err(Error::Custom(msg));
    }

    transaction.set_remove(packages);

    let assume_yes = options.contains(&SyncOption::AssumeYes);
    if !assume_yes {
        if let Some(tx) = session.emitter() {
            if tx
                .send(Event::PromptTransactionNeedConfirm(transaction.clone()))
                .is_ok()
            {
                let rx = session.receiver().unwrap();
                let mut confirmed = false;

                while let Ok(event) = rx.recv() {
                    if let Event::PromptTransactionNeedConfirmResult(ret) = event {
                        confirmed = ret;
                        break;
                    }
                }

                if !confirmed {
                    return Ok(());
                }
            }
        }
    }

    if let Some(packages) = transaction.remove_view() {
        let purge = options.contains(&SyncOption::Purge);
        let config = session.config();
        let root_dir = config.root_path();

        for package in packages.iter() {
            if let Some(tx) = session.emitter() {
                let _ = tx.send(Event::PackageCommitStart(package.name().to_owned()));
            }

            let app_dir = root_dir.join("apps").join(package.name());

            // TODO: pre_uninstall
            // TODO: uninstaller

            shim::remove(session, package)?;
            let _ = shortcut::remove(session, package);
            let _ = psmodule::remove(session, package);
            let _ = env::remove(session, package);
            let _ = persist::unlink(session, package);

            let current_lnk = app_dir.join("current");
            let _ = internal::fs::remove_symlink(current_lnk);

            // TODO: post_uninstall

            // Remove the app directory
            std::fs::remove_dir_all(&app_dir)?;

            if purge {
                if let Some(tx) = session.emitter() {
                    let _ = tx.send(Event::PackagePersistPurgeStart);
                }

                let persist_dir = config.root_path().join("persist").join(package.name());
                internal::fs::remove_dir(persist_dir)?;

                if let Some(tx) = session.emitter() {
                    let _ = tx.send(Event::PackagePersistPurgeDone);
                }
            }

            if let Some(tx) = session.emitter() {
                let _ = tx.send(Event::PackageCommitDone(package.name().to_owned()));
            }
        }
    }

    Ok(())
}
