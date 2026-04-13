use solana_transaction_status_client_types::UiTransaction;
use yellowstone_grpc_proto::prelude::SubscribeUpdateTransaction;

#[derive(Clone)]
pub enum TransactionData {
    UiTransaction(UiTransaction),
    GrpcTransaction(SubscribeUpdateTransaction),
}
