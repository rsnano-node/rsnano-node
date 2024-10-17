use rsnano_node::wallets::WalletsError;
use serde::{ser::SerializeMap, Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub enum ErrorDto {
    WalletsError(WalletsError),
    RPCControlDisabled,
    AccountNotFound,
    BlockNotFound,
    PeerNotFound,
    BlockError,
    MissingAccountInformation,
    WorkLow,
    GapPrevious,
    GapSource,
    Old,
    BadSignature,
    NegativeSpend,
    BalanceMismatch,
    Unreceivable,
    BlockPosition,
    GapEpochOpenPending,
    Fork,
    InsufficientWork,
    OpenedBurnAccount,
    Other,
    Stopped,
    NotStateBlock,
    LegacyBootstrapDisabled,
    LazyBootstrapDisabled,
    ConfirmationInfoNotFound,
    InvalidRoot,
    DifficultyOutOfRange,
    BlockRootMismatch,
    BlockWorkVersioMismatch,
    AccountHeadNotFound,
    InsufficientBalance,
}

impl Serialize for ErrorDto {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let error_message = match self {
            ErrorDto::WalletsError(e) => e.to_string(),
            ErrorDto::RPCControlDisabled => "RPC control is disabled".to_string(),
            ErrorDto::AccountNotFound => "Account not found".to_string(),
            ErrorDto::BlockNotFound => "Block not found".to_string(),
            ErrorDto::PeerNotFound => "Peer not found".to_string(),
            ErrorDto::BlockError => "Block error".to_string(),
            ErrorDto::MissingAccountInformation => "Missing account information".to_string(),
            ErrorDto::WorkLow => "Work low".to_string(),
            ErrorDto::GapPrevious => "Gap previous".to_string(),
            ErrorDto::GapSource => "Gap source".to_string(),
            ErrorDto::Old => "Old".to_string(),
            ErrorDto::BadSignature => "Bad signature".to_string(),
            ErrorDto::NegativeSpend => "Negative spend".to_string(),
            ErrorDto::BalanceMismatch => "Balance mismatch".to_string(),
            ErrorDto::Unreceivable => "Unreceivable".to_string(),
            ErrorDto::BlockPosition => "Block position".to_string(),
            ErrorDto::GapEpochOpenPending => "Gap epoch open pending".to_string(),
            ErrorDto::Fork => "Fork".to_string(),
            ErrorDto::InsufficientWork => "Insufficient work".to_string(),
            ErrorDto::OpenedBurnAccount => "Opened burn account".to_string(),
            ErrorDto::Other => "Other".to_string(),
            ErrorDto::Stopped => "Stopped".to_string(),
            ErrorDto::NotStateBlock => "Is not state block".to_string(),
            ErrorDto::LegacyBootstrapDisabled => "Legacy bootstrap is disabled".to_string(),
            ErrorDto::LazyBootstrapDisabled => "Lazy bootstrap is disabled".to_string(),
            ErrorDto::ConfirmationInfoNotFound => "Confirmation info not found".to_string(),
            ErrorDto::InvalidRoot => "Invalid root".to_string(),
            ErrorDto::DifficultyOutOfRange => "Difficulty out of valid range".to_string(),
            ErrorDto::BlockRootMismatch => "Block root mismatch".to_string(),
            ErrorDto::BlockWorkVersioMismatch => "Block work version mismatch".to_string(),
            ErrorDto::AccountHeadNotFound => "Account head not found".to_string(),
            ErrorDto::InsufficientBalance => "Insufficient balance".to_string(),
        };

        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("error", &error_message)?;
        map.end()
    }
}
