use std::time::Duration;

/// Builds an HTTP agent that uses the platform TLS stack. A timeout applies to
/// connecting and to each read and write, so a slow transfer is not cut short.
pub(crate) fn agent(timeout: Option<Duration>) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .build();
    let mut builder = ureq::config::Config::builder().tls_config(tls);
    if timeout.is_some() {
        builder = builder
            .timeout_connect(timeout)
            .timeout_per_read(timeout)
            .timeout_per_write(timeout);
    }
    builder.build().into()
}
