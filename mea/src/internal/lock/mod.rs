// Copyright 2024 tison <wander4096@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Synchronized lock primitives for internal usage.
//!
//! Although this crate provides async utilities, to implement them properly, sync primitives are
//! still needed. Sync primitives are used internally only and mainly for exclusive access to shared
//! resources. All the guards are expected to release the resources as soon as updates are done, so
//! that there is no need to worry about long blocking and never deadlocks.

#[cfg(all(not(feature = "std"), feature = "sync-critical-section"))]
mod impl_critical_section;
#[cfg(all(not(feature = "std"), feature = "sync-critical-section"))]
pub(crate) use impl_critical_section::Mutex;

#[cfg(all(not(feature = "std"), feature = "sync-futex"))]
mod impl_futex;
#[cfg(all(not(feature = "std"), feature = "sync-futex"))]
pub(crate) use impl_futex::Mutex;

#[cfg(feature = "std")]
mod impl_std;
#[cfg(feature = "std")]
pub(crate) use impl_std::Mutex;

#[cfg(test)]
mod tests;
