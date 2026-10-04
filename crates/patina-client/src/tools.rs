use crate::{Client, ClientError, REQUEST_TIMEOUT};
use patina_protocol::tools::*;
use serde::Serialize;

impl Client {
    pub async fn tools_snapshot(&self) -> Result<ToolsRuntimeSnapshot, ClientError> {
        self.get_json_with_limits(
            "/api/v1/tools/snapshot",
            "Tools snapshot",
            REQUEST_TIMEOUT,
            MAX_TOOLS_RESPONSE_BYTES,
        )
        .await
    }

    async fn tools_write<T: Serialize + ?Sized>(
        &self,
        path: &str,
        request: &T,
    ) -> Result<ToolsRuntimeSnapshot, ClientError> {
        let capabilities = self.capabilities().await?;
        if !capabilities.tools.owned
            || !capabilities.tools.ready
            || !capabilities.write_api.available
            || !capabilities
                .write_api
                .operations
                .iter()
                .any(|operation| operation == "tools")
        {
            return Err(ClientError::UnsupportedCapability("tools".into()));
        }
        crate::negotiate_tracking_capabilities(capabilities)?;
        self.post_json_with_limits(
            path,
            request,
            "Tools action",
            REQUEST_TIMEOUT,
            MAX_TOOLS_RESPONSE_BYTES,
        )
        .await
    }

    pub async fn create_reminder(
        &self,
        request: &CreateReminderRequest,
    ) -> Result<ToolsRuntimeSnapshot, ClientError> {
        self.tools_write("/api/v1/tools/reminders", request).await
    }

    pub async fn cancel_reminder(&self, id: i64) -> Result<ToolsRuntimeSnapshot, ClientError> {
        require_positive_id(id)?;
        self.tools_write(
            &format!("/api/v1/tools/reminders/{id}/cancel"),
            &serde_json::json!({}),
        )
        .await
    }

    pub async fn create_software_reminder_rule(
        &self,
        request: &CreateSoftwareReminderRuleRequest,
    ) -> Result<ToolsRuntimeSnapshot, ClientError> {
        self.tools_write("/api/v1/tools/software-reminder-rules", request)
            .await
    }

    pub async fn disable_software_reminder_rule(
        &self,
        id: i64,
    ) -> Result<ToolsRuntimeSnapshot, ClientError> {
        require_positive_id(id)?;
        self.tools_write(
            &format!("/api/v1/tools/software-reminder-rules/{id}/disable"),
            &serde_json::json!({}),
        )
        .await
    }

    pub async fn start_timer(
        &self,
        request: &StartTimerRequest,
    ) -> Result<ToolsRuntimeSnapshot, ClientError> {
        self.tools_write("/api/v1/tools/timer/start", request).await
    }

    pub async fn start_pomodoro(
        &self,
        request: &StartPomodoroRequest,
    ) -> Result<ToolsRuntimeSnapshot, ClientError> {
        self.tools_write("/api/v1/tools/pomodoro/start", request)
            .await
    }

    pub async fn tools_action(
        &self,
        action: ToolsAction,
    ) -> Result<ToolsRuntimeSnapshot, ClientError> {
        self.tools_write(action.path(), &serde_json::json!({}))
            .await
    }
}

fn require_positive_id(id: i64) -> Result<(), ClientError> {
    if id <= 0 {
        return Err(ClientError::InvalidConfiguration(
            "Tools id must be positive".into(),
        ));
    }
    Ok(())
}
