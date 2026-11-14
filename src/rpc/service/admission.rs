use tonic::{Request, Status};

use super::{RuntimeService, status};
use crate::remote::Origin;

impl RuntimeService {
    /// The message of `request` once its model selector is one its caller
    /// may name: any path for the local socket, but for a remote key only a
    /// configured key or a model path the server lists itself.
    pub(super) async fn admitted<T>(
        &self,
        request: Request<T>,
        selector: fn(&T) -> &str,
    ) -> Result<T, Status> {
        let origin = Origin::of(&request);
        let message = request.into_inner();
        if origin.is_remote() {
            let application = self.application.clone();
            let selector = selector(&message).to_owned();
            status::internal(
                tokio::task::spawn_blocking(move || application.check_offered(&selector)).await,
            )?
            .map_err(status::application)?;
        }
        Ok(message)
    }
}
