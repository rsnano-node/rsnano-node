pub mod nano {
    pub mod v1 {
        tonic::include_proto!("nano.v1");

        pub const FILE_DESCRIPTOR_SET: &[u8] =
            tonic::include_file_descriptor_set!("file_descriptor_set");
    }
}
