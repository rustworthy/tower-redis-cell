mod command;

pub use command::{RedisCellCommand, RedisCellCommandBuilder};

#[cfg(test)]
mod tests {
    use redis::Cmd;
    use std::time::Duration;
    use testcontainers::core::IntoContainerPort as _;
    use testcontainers::runners::AsyncRunner;
    use testcontainers::{GenericImage, core::WaitFor};

    #[tokio::test]
    async fn it_works_with_redis() {
        let container = GenericImage::new("redis-cell", "latest")
            .with_exposed_port(6379.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
            .start()
            .await
            .unwrap();
        let port = container.get_host_port_ipv4(6379).await.unwrap();
        let client = redis::Client::open(("localhost", port)).unwrap();
        let config = redis::aio::ConnectionManagerConfig::new().set_number_of_retries(1);
        let mut manager = redis::aio::ConnectionManager::new_with_config(client, config)
            .await
            .unwrap();

        let mut builder = super::RedisCellCommand::builder("user123");
        let cmd: Cmd = builder
            .burst(1usize)
            .tokens(10usize)
            .period(Duration::from_secs(60))
            .apply(1usize)
            .build()
            .into();
        let res = manager.send_packed_command(&cmd).await.unwrap();
        dbg!(res);
        let res = manager.send_packed_command(&cmd).await.unwrap();
        dbg!(res);
        let res = manager.send_packed_command(&cmd).await.unwrap();
        dbg!(res);
    }

    #[tokio::test]
    async fn it_works_with_valkey() {
        let container = GenericImage::new("valkey-cell", "latest")
            .with_exposed_port(6379.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
            .start()
            .await
            .unwrap();
        let port = container.get_host_port_ipv4(6379).await.unwrap();
        let client = redis::Client::open(("localhost", port)).unwrap();
        let config = redis::aio::ConnectionManagerConfig::new().set_number_of_retries(1);
        let mut manager = redis::aio::ConnectionManager::new_with_config(client, config)
            .await
            .unwrap();

        let mut builder = super::RedisCellCommand::builder("user123");
        let cmd: Cmd = builder
            .burst(1usize)
            .tokens(10usize)
            .period(Duration::from_secs(60))
            .apply(1usize)
            .build()
            .into();
        let res = manager.send_packed_command(&cmd).await.unwrap();
        dbg!(res);
        let res = manager.send_packed_command(&cmd).await.unwrap();
        dbg!(res);
        let res = manager.send_packed_command(&cmd).await.unwrap();
        dbg!(res);
    }
}
