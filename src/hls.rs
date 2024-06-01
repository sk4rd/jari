use std::sync::Arc;

use tokio::time::Instant;

use crate::AppState;

/// Function to add the new segments and set the new current segment
pub async fn update(_instant: Instant, _data: Arc<AppState>) {
    // TODO: Update the HLS data on to instant
    println!("{}µs", _instant.elapsed().as_micros())
}
