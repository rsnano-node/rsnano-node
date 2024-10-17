use rsnano_core::{UncheckedInfo, UncheckedKey};
use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, HashRpcMessage, RpcDto, UncheckedGetDto};
use std::sync::{Arc, Mutex};

pub async fn unchecked_get(node: Arc<Node>, args: HashRpcMessage) -> RpcDto {
    let result = Arc::new(Mutex::new(None));

    node.unchecked.for_each(
        {
            let result = Arc::clone(&result);
            Box::new(move |key: &UncheckedKey, info: &UncheckedInfo| {
                if key.hash == args.hash {
                    let modified_timestamp = info.modified;
                    if let Some(block) = info.block.as_ref() {
                        let contents = block.json_representation();
                        let mut result_guard = result.lock().unwrap();
                        *result_guard = Some(UncheckedGetDto::new(modified_timestamp, contents));
                    }
                }
            })
        },
        Box::new(|| true),
    );

    let result = result.lock().unwrap().take();
    result.map_or_else(
        || RpcDto::Error(ErrorDto::BlockNotFound),
        |dto| RpcDto::UncheckedGet(dto),
    )
}
