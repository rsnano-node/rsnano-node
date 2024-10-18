use rsnano_core::{UncheckedInfo, UncheckedKey};
use rsnano_node::Node;
use rsnano_rpc_messages::{RpcDto, UncheckedKeyDto, UncheckedKeysArgs, UncheckedKeysDto};
use std::sync::{Arc, Mutex};

pub async fn unchecked_keys(node: Arc<Node>, args: UncheckedKeysArgs) -> RpcDto {
    let unchecked_keys = Arc::new(Mutex::new(Vec::new()));

    node.unchecked.for_each_with_dependency(
        &args.key,
        &mut {
            let unchecked_keys = Arc::clone(&unchecked_keys);
            move |unchecked_key: &UncheckedKey, info: &UncheckedInfo| {
                let mut unchecked_keys_guard = unchecked_keys.lock().unwrap();
                if unchecked_keys_guard.len() < args.count as usize {
                    if let Some(block) = info.block.as_ref() {
                        let dto = UncheckedKeyDto::new(
                            unchecked_key.hash,
                            block.hash(),
                            info.modified,
                            block.json_representation(),
                        );
                        unchecked_keys_guard.push(dto);
                    }
                }
            }
        },
        &Box::new(|| unchecked_keys.lock().unwrap().len() < args.count as usize),
    );

    let unchecked_keys = Arc::try_unwrap(unchecked_keys)
        .unwrap()
        .into_inner()
        .unwrap();

    RpcDto::UncheckedKeys(UncheckedKeysDto::new(unchecked_keys))
}
