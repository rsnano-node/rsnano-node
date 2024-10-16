use anyhow::Context;
use rsnano_core::{KeyPair, KeyPairFactory};
use rsnano_nullable_fs::NullableFilesystem;
use std::path::{Path, PathBuf};
use tracing::info;

/// The node creates a file called 'node_id_private.key' on its first start.
/// In this file the private key of the node id is stored.
#[derive(Default)]
pub(crate) struct NodeIdKeyFile {
    key_factory: KeyPairFactory,
    fs: NullableFilesystem,
}

impl NodeIdKeyFile {
    #[allow(dead_code)]
    fn new(fs: NullableFilesystem, key_factory: KeyPairFactory) -> Self {
        Self { fs, key_factory }
    }

    pub fn new_null() -> Self {
        Self::new(NullableFilesystem::new_null(), KeyPairFactory::new_null())
    }

    pub fn initialize(&mut self, app_path: impl AsRef<Path>) -> anyhow::Result<KeyPair> {
        let app_path = app_path.as_ref();
        let file_path = Self::key_file_path(app_path);
        if self.fs.exists(&file_path) {
            self.load_key(&file_path)
        } else {
            self.create_key(app_path, &file_path)
        }
    }

    fn key_file_path(app_path: &Path) -> PathBuf {
        let mut key_file = PathBuf::from(app_path);
        key_file.push("node_id_private.key");
        key_file
    }

    fn load_key(&mut self, file_path: &Path) -> anyhow::Result<KeyPair> {
        info!("Reading node id from: {:?}", file_path);

        let content = self
            .fs
            .read_to_string(&file_path)
            .context(format!("Could not read node id file {:?}", file_path))?;

        let first_line = content.lines().next().unwrap_or("");
        KeyPair::from_priv_key_hex(first_line).context(format!(
            "Could not decode node id key from file {:?}",
            file_path
        ))
    }

    fn create_key(&mut self, app_path: &Path, file_path: &Path) -> anyhow::Result<KeyPair> {
        info!("Generating a new node id, saving to: {:?}", file_path);

        self.fs
            .create_dir_all(app_path)
            .context(format!("Could not create app dir: {:?}", app_path))?;

        let keypair = self.key_factory.create_key_pair();

        self.fs
            .write(
                file_path,
                format!("{}\n", keypair.private_key().encode_hex()).as_bytes(),
            )
            .context(format!("Could not write node id key file: {:?}", file_path))?;

        Ok(keypair)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_core::RawKey;
    use rsnano_nullable_fs::FsEvent;
    use std::io::ErrorKind;
    use tracing_test::traced_test;

    static EXPECTED_KEY: RawKey = RawKey::from_bytes([42; 32]);

    #[test]
    fn node_id_key_file_path() {
        assert_eq!(
            NodeIdKeyFile::key_file_path(&PathBuf::from("/path/to/node")),
            PathBuf::from("/path/to/node/node_id_private.key")
        );
    }

    mod key_file_exists {
        use super::*;

        #[test]
        fn load_key_from_file() {
            let (key_pair, _) = initialize_node_id_with_valid_existing_file();
            assert_eq!(key_pair.unwrap().private_key(), EXPECTED_KEY);
        }

        #[test]
        fn dont_change_the_filesystem() {
            let (_, fs_events) = initialize_node_id_with_valid_existing_file();
            assert!(fs_events.is_empty());
        }

        #[test]
        #[traced_test]
        fn log_reading_file() {
            let _ = initialize_node_id_with_valid_existing_file();
            assert!(logs_contain(
                "Reading node id from: \"/path/to/node/node_id_private.key\""
            ));
        }

        #[test]
        fn fail_if_file_is_inaccessable() {
            let fs = fs_with_inaccessable_key_file();
            let (Err(err), _) = initialize_node_id(fs, KeyPairFactory::new_null()) else {
                panic!("initialization should fail")
            };
            assert_eq!(
                err.to_string(),
                "Could not read node id file \"/path/to/node/node_id_private.key\""
            )
        }

        #[test]
        fn fail_if_file_cannot_be_parsed() {
            let fs = fs_with_key_file("invalid file content");
            let (Err(err), _) = initialize_node_id(fs, KeyPairFactory::new_null()) else {
                panic!("initialization should fail")
            };
            assert_eq!(
                err.to_string(),
                "Could not decode node id key from file \"/path/to/node/node_id_private.key\""
            )
        }
    }

    mod no_file_exists_yet {
        use super::*;

        #[test]
        fn create_new_node_id() {
            let (key_pair, _) = initialize_node_id_without_existing_file();
            assert_eq!(key_pair.unwrap().private_key(), EXPECTED_KEY);
        }

        #[test]
        fn create_file_with_private_key() {
            let (_, fs_events) = initialize_node_id_without_existing_file();
            assert_eq!(fs_events.len(), 2);
            assert_eq!(fs_events[0], FsEvent::create_dir_all(test_app_path()));
            assert_eq!(
                fs_events[1],
                FsEvent::write(
                    test_key_file_path(),
                    format!("{}\n", EXPECTED_KEY.encode_hex())
                )
            );
        }

        #[test]
        #[traced_test]
        fn log_file_creation() {
            let _ = initialize_node_id_without_existing_file();
            assert!(logs_contain(
                "Generating a new node id, saving to: \"/path/to/node/node_id_private.key\""
            ));
        }

        #[test]
        fn fail_if_directory_cannot_be_created() {
            let fs = NullableFilesystem::null_builder()
                .create_dir_all_fails(
                    test_app_path(),
                    std::io::Error::new(ErrorKind::PermissionDenied, ""),
                )
                .finish();

            let (Err(err), _) = initialize_node_id(fs, KeyPairFactory::new_null()) else {
                panic!("should fail");
            };
            assert_eq!(
                err.to_string(),
                "Could not create app dir: \"/path/to/node\""
            );
        }

        #[test]
        fn fail_if_file_cannot_be_written() {
            let fs = NullableFilesystem::null_builder()
                .write_fails(
                    test_key_file_path(),
                    std::io::Error::new(ErrorKind::PermissionDenied, ""),
                )
                .finish();

            let (Err(err), _) = initialize_node_id(fs, KeyPairFactory::new_null()) else {
                panic!("should fail");
            };
            assert_eq!(
                err.to_string(),
                "Could not write node id key file: \"/path/to/node/node_id_private.key\""
            );
        }
    }

    fn initialize_node_id_without_existing_file() -> (anyhow::Result<KeyPair>, Vec<FsEvent>) {
        let fs = NullableFilesystem::new_null();
        let key_pair_factory = KeyPairFactory::new_null_with(EXPECTED_KEY.clone());
        initialize_node_id(fs, key_pair_factory)
    }

    fn initialize_node_id_with_valid_existing_file() -> (anyhow::Result<KeyPair>, Vec<FsEvent>) {
        let fs = fs_with_key_file(format!("{}\n", EXPECTED_KEY.encode_hex()));
        initialize_node_id(fs, KeyPairFactory::new_null())
    }

    fn initialize_node_id(
        fs: NullableFilesystem,
        key_pair_factory: KeyPairFactory,
    ) -> (anyhow::Result<KeyPair>, Vec<FsEvent>) {
        let fs_tracker = fs.track();
        let mut id_file = NodeIdKeyFile::new(fs, key_pair_factory);
        let key_pair = id_file.initialize(&test_app_path());
        let fs_events = fs_tracker.output();
        (key_pair, fs_events)
    }

    fn test_app_path() -> PathBuf {
        PathBuf::from("/path/to/node")
    }

    fn test_key_file_path() -> PathBuf {
        NodeIdKeyFile::key_file_path(&test_app_path())
    }

    fn fs_with_inaccessable_key_file() -> NullableFilesystem {
        let file_path = test_key_file_path();
        NullableFilesystem::null_builder()
            .path_exists(&file_path)
            .read_to_string_fails(
                &file_path,
                std::io::Error::new(ErrorKind::PermissionDenied, ""),
            )
            .finish()
    }

    fn fs_with_key_file(contents: impl Into<String>) -> NullableFilesystem {
        let file_path = test_key_file_path();
        NullableFilesystem::null_builder()
            .path_exists(&file_path)
            .read_to_string(&file_path, contents.into())
            .finish()
    }
}
