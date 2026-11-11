use std::time::Duration;

use tokio::process::Child;

pub async fn stop(child: &mut Child) {
    // SIGKILL bypasses LLVM's profile flush and cannot verify graceful shutdown.
    #[cfg(unix)]
    {
        let status = tokio::process::Command::new("kill")
            .args(["-TERM", &child.id().unwrap().to_string()])
            .status()
            .await
            .unwrap();
        assert!(status.success());
        assert!(
            tokio::time::timeout(Duration::from_secs(10), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
    }
    #[cfg(not(unix))]
    {
        child.kill().await.unwrap();
        child.wait().await.unwrap();
    }
}
