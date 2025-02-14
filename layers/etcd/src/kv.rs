// Copyright 2023 The Sekas Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use tonic::{Request, Response, Status};

use crate::etcd::v3::{kv_server, *};

type Result<T> = std::result::Result<T, Status>;

pub struct Kv {}

#[tonic::async_trait]
impl kv_server::Kv for Kv {
    /// Range gets the keys in the range from the key-value store.
    async fn range(&self, _request: Request<RangeRequest>) -> Result<Response<RangeResponse>> {
        todo!()
    }

    /// Put puts the given key into the key-value store.
    /// A put request increments the revision of the key-value store
    /// and generates one event in the event history.
    async fn put(&self, _request: Request<PutRequest>) -> Result<Response<PutResponse>> {
        todo!()
    }

    /// DeleteRange deletes the given range from the key-value store.
    /// A delete request increments the revision of the key-value store
    /// and generates a delete event in the event history for every deleted key.
    async fn delete_range(
        &self,
        _request: Request<DeleteRangeRequest>,
    ) -> Result<Response<DeleteRangeResponse>> {
        todo!()
    }

    /// Txn processes multiple requests in a single transaction.
    /// A txn request increments the revision of the key-value store
    /// and generates events with the same revision for every completed request.
    /// It is not allowed to modify the same key several times within one txn.
    async fn txn(&self, _request: Request<TxnRequest>) -> Result<Response<TxnResponse>> {
        todo!()
    }
}
