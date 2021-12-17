# Rust Server with Switchable Transport

This is a example echo server in Rust, that shows how to implement switchable transport using generics.

With generics, there is no runtime cost of dynamical dispatch, but it results in a larger binary. It's also less flexible.

Other approachs to achieve switchable transport layer are enum_dispatch and dynamical dispatch using `Box` and `dyn`. These methods are easier to implement, but all come with runtime cost.

Hopefully you can find this helpful :)

## Usage

Run the server in TLS
```
cargo run -- tls server
# In a new terminal
cargo run -- tls client
```

Run the server in TCP
```
cargo run -- tcp server
# In a new terminal
cargo run -- tcp client
```

They use the same code, but different transport layers!
```rust
async fn send_hello<T: Transport>(transport: T) -> Result<()> {
    let mut conn = transport.connect(&String::from("127.0.0.1:2334")).await?;
    let req = "hello";
    conn.write_all(req.as_bytes()).await?;
    io::copy(&mut conn, &mut io::stdout()).await?;
    Ok(())
}
```

# WARNING
Don't ever use the come-along certifacte in production! It's just for the presentation.
