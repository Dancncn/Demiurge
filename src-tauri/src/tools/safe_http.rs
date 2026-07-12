//! SSRF-safe HTTP primitives for tools that are allowed to read public URLs.
//!
//! Every hop resolves and validates its host before connecting. The validated
//! addresses are then installed as reqwest DNS overrides so a second DNS lookup
//! cannot swap in a private address between validation and the TCP connection.

use reqwest::{
    header::{HeaderValue, LOCATION},
    redirect::Policy,
    Client, Response, StatusCode, Url,
};
use std::{
    collections::HashSet,
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    time::Duration,
};

const MAX_REDIRECTS: usize = 10;
const DNS_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug)]
struct ResolvedTarget {
    domain: Option<String>,
    addresses: Vec<SocketAddr>,
}

/// Sends a GET request that may only connect to public internet addresses.
///
/// Automatic redirects and environment proxies are disabled. Redirects are
/// followed manually so every `Location` host gets a fresh DNS/public-address
/// check and a new pinned client.
pub(super) async fn get_public(
    url: &str,
    user_agent: &'static str,
    accept: &str,
) -> Result<Response, String> {
    get_public_with_sender(url, move |current| {
        send_public_hop(current, user_agent, accept)
    })
    .await
}

/// Runs the redirect state machine with one caller-supplied hop sender.
/// Production always supplies `send_public_hop`; the seam exists so tests can
/// serve a synthetic first public hop locally and prove the next private hop is
/// rejected by the real resolver before any second connection is attempted.
async fn get_public_with_sender<F, Fut>(url: &str, mut send_hop: F) -> Result<Response, String>
where
    F: FnMut(Url) -> Fut,
    Fut: Future<Output = Result<Response, String>>,
{
    let mut current = Url::parse(url).map_err(|e| format!("URL 无效：{e}"))?;
    validate_url_shape(&current)?;
    let mut redirects = 0;

    loop {
        let response = send_hop(current.clone()).await?;

        let Some(next) = redirect_target(
            &current,
            response.status(),
            response.headers().get(LOCATION),
        )?
        else {
            return Ok(response);
        };

        if redirects >= MAX_REDIRECTS {
            return Err(format!("公开 URL 重定向超过 {MAX_REDIRECTS} 次"));
        }
        redirects += 1;
        current = next;
    }
}

async fn send_public_hop(
    current: Url,
    user_agent: &'static str,
    accept: &str,
) -> Result<Response, String> {
    let resolved = resolve_target(&current).await?;
    let client = build_pinned_client(&resolved)?;
    let response = client
        .get(current)
        .header("User-Agent", user_agent)
        .header("Accept", accept)
        .send()
        .await
        .map_err(|e| format!("连接公开 URL 失败：{e}"))?;

    verify_remote_address(&response, &resolved)?;
    Ok(response)
}

pub(super) fn validate_url_shape(url: &Url) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("只允许访问公开 http/https URL".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("公开 URL 不允许包含用户名或密码".to_string());
    }
    if url.host_str().is_none() {
        return Err("公开 URL 必须包含主机名".to_string());
    }
    if url.port_or_known_default().is_none() {
        return Err("公开 URL 缺少有效端口".to_string());
    }
    Ok(())
}

async fn resolve_target(url: &Url) -> Result<ResolvedTarget, String> {
    validate_url_shape(url)?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "公开 URL 缺少有效端口".to_string())?;

    if let Some(domain) = url.domain() {
        let domain = domain.to_string();
        let lookup_domain = domain.clone();
        let lookup = tokio::task::spawn_blocking(move || {
            (lookup_domain.as_str(), port)
                .to_socket_addrs()
                .map(|iter| iter.collect::<Vec<_>>())
        });
        let addresses = tokio::time::timeout(DNS_TIMEOUT, lookup)
            .await
            .map_err(|_| format!("解析主机 {domain} 超时"))?
            .map_err(|e| format!("解析主机 {domain} 的任务失败：{e}"))?
            .map_err(|e| format!("无法解析主机 {domain}：{e}"))?;
        let addresses = validate_resolved_addresses(&domain, addresses)?;
        return Ok(ResolvedTarget {
            domain: Some(domain),
            addresses,
        });
    }

    let host = url
        .host_str()
        .ok_or_else(|| "公开 URL 必须包含主机名".to_string())?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let ip = host
        .parse::<IpAddr>()
        .map_err(|_| format!("无法解析 URL 主机：{host}"))?;
    let addresses = validate_resolved_addresses(host, vec![SocketAddr::new(ip, port)])?;
    Ok(ResolvedTarget {
        domain: None,
        addresses,
    })
}

fn validate_resolved_addresses(
    host: &str,
    addresses: Vec<SocketAddr>,
) -> Result<Vec<SocketAddr>, String> {
    let mut seen = HashSet::new();
    let mut validated = Vec::new();

    for address in addresses {
        let ip = canonical_ip(address.ip());
        if !is_public_ip(ip) {
            return Err(format!("拒绝访问非公网地址：主机 {host} 解析到 {ip}"));
        }
        if seen.insert(ip) {
            validated.push(SocketAddr::new(ip, address.port()));
        }
    }

    if validated.is_empty() {
        return Err(format!("主机 {host} 没有可用的公网地址"));
    }
    Ok(validated)
}

fn build_pinned_client(target: &ResolvedTarget) -> Result<Client, String> {
    let mut builder = Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .timeout(REQUEST_TIMEOUT);
    if let Some(domain) = &target.domain {
        builder = builder.resolve_to_addrs(domain, &target.addresses);
    }
    builder
        .build()
        .map_err(|e| format!("创建公开 URL 客户端失败：{e}"))
}

fn verify_remote_address(response: &Response, target: &ResolvedTarget) -> Result<(), String> {
    let Some(remote) = response.remote_addr() else {
        // DNS is still pinned to the validated set. Some connectors/platforms do
        // not expose the peer address, so absence alone must not break requests.
        return Ok(());
    };
    let remote_ip = canonical_ip(remote.ip());
    if !is_public_ip(remote_ip)
        || !target
            .addresses
            .iter()
            .any(|approved| canonical_ip(approved.ip()) == remote_ip)
    {
        return Err(format!("连接落到未经校验的地址：{remote_ip}"));
    }
    Ok(())
}

fn redirect_target(
    current: &Url,
    status: StatusCode,
    location: Option<&HeaderValue>,
) -> Result<Option<Url>, String> {
    if !matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    ) {
        return Ok(None);
    }

    let Some(location) = location else {
        return Ok(None);
    };
    let location = location
        .to_str()
        .map_err(|_| "重定向 Location 不是有效文本".to_string())?;
    let next = current
        .join(location)
        .map_err(|e| format!("重定向 URL 无效：{e}"))?;
    validate_url_shape(&next)?;
    Ok(Some(next))
}

fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ipv6) => ipv6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ipv6)),
        IpAddr::V4(_) => ip,
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => is_public_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_public_ipv6(ipv6),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(
        // "This network", private, shared carrier NAT, loopback, and link-local.
        a == 0
            || a == 10
            || (a == 100 && (64..=127).contains(&b))
            || a == 127
            || (a == 169 && b == 254)
            || (a == 172 && (16..=31).contains(&b))
            // IETF protocol assignments, documentation, deprecated relay,
            // private, benchmarking, and documentation networks.
            || (a == 192 && b == 0 && c == 0)
            || (a == 192 && b == 0 && c == 2)
            || (a == 192 && b == 88 && c == 99)
            || (a == 192 && b == 168)
            || (a == 198 && (18..=19).contains(&b))
            || (a == 198 && b == 51 && c == 100)
            || (a == 203 && b == 0 && c == 113)
            // Multicast, reserved future-use space, and limited broadcast.
            || a >= 224
    )
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(ipv4) = ip.to_ipv4_mapped() {
        return is_public_ipv4(ipv4);
    }

    let value = u128::from(ip);
    // Currently allocated global-unicast space is 2000::/3. Treat all other
    // space as non-public so unspecified, loopback, ULA, link/site-local,
    // multicast, translation, discard-only, and unallocated ranges fail closed.
    if !in_ipv6_prefix(
        value,
        u128::from(Ipv6Addr::new(0x2000, 0, 0, 0, 0, 0, 0, 0)),
        3,
    ) {
        return false;
    }

    // Special-purpose ranges located inside 2000::/3.
    !in_ipv6_prefix(
        value,
        u128::from(Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0)),
        23,
    ) && !in_ipv6_prefix(
        value,
        u128::from(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 0)),
        32,
    ) && !in_ipv6_prefix(
        value,
        u128::from(Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0)),
        16,
    ) && !in_ipv6_prefix(
        value,
        u128::from(Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0)),
        20,
    ) && !in_ipv6_prefix(
        value,
        u128::from(Ipv6Addr::new(0x3ffe, 0, 0, 0, 0, 0, 0, 0)),
        16,
    )
}

fn in_ipv6_prefix(address: u128, network: u128, prefix_len: u32) -> bool {
    let mask = if prefix_len == 0 {
        0
    } else {
        u128::MAX << (128 - prefix_len)
    };
    address & mask == network & mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread,
    };

    #[test]
    fn rejects_non_public_ipv4_ranges() {
        for address in [
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "192.88.99.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.255",
            "240.0.0.1",
            "255.255.255.255",
        ] {
            let ip = address.parse::<IpAddr>().unwrap();
            assert!(!is_public_ip(ip), "{address} must be rejected");
        }
        for address in ["1.1.1.1", "8.8.8.8", "192.31.196.1"] {
            let ip = address.parse::<IpAddr>().unwrap();
            assert!(is_public_ip(ip), "{address} must be accepted");
        }
    }

    #[test]
    fn rejects_non_public_ipv6_ranges_and_mapped_private_ips() {
        for address in [
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::808:808",
            "100::1",
            "2001::1",
            "2001:db8::1",
            "2002::1",
            "3fff::1",
            "3ffe::1",
            "fc00::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
        ] {
            let ip = address.parse::<IpAddr>().unwrap();
            assert!(!is_public_ip(ip), "{address} must be rejected");
        }
        for address in ["2001:4860:4860::8888", "2606:4700:4700::1111"] {
            let ip = address.parse::<IpAddr>().unwrap();
            assert!(is_public_ip(ip), "{address} must be accepted");
        }
    }

    #[test]
    fn rejects_credentials_and_mixed_dns_answers() {
        let credentialed = Url::parse("https://user:secret@example.com/").unwrap();
        assert!(validate_url_shape(&credentialed).is_err());

        let addresses = vec![
            SocketAddr::from(([8, 8, 8, 8], 443)),
            SocketAddr::from(([127, 0, 0, 1], 443)),
        ];
        assert!(validate_resolved_addresses("mixed.example", addresses).is_err());
    }

    #[tokio::test]
    async fn rejects_literal_and_resolved_local_hosts() {
        let literal = Url::parse("http://127.0.0.1/").unwrap();
        assert!(resolve_target(&literal).await.is_err());

        let ipv6 = Url::parse("http://[::1]/").unwrap();
        assert!(resolve_target(&ipv6).await.is_err());

        let localhost = Url::parse("http://localhost/").unwrap();
        assert!(resolve_target(&localhost).await.is_err());
    }

    #[tokio::test]
    async fn rejects_alternative_ipv4_notation_and_mapped_ipv6_urls() {
        for raw in [
            "http://127.1/",
            "http://2130706433/",
            "http://0x7f000001/",
            "http://017700000001/",
            "http://0x7f.0.0.1/",
            "http://0177.0.0.1/",
            "http://[::ffff:127.0.0.1]/",
        ] {
            let url = Url::parse(raw).unwrap_or_else(|error| panic!("{raw}: {error}"));
            assert!(
                resolve_target(&url).await.is_err(),
                "alternative local address must be rejected: {raw} normalized to {url}"
            );
        }
    }

    #[tokio::test]
    async fn pinned_client_uses_validated_dns_and_never_auto_redirects() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1/blocked\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });

        // Construction bypasses address validation only inside this unit test.
        // Success proves the fake hostname is resolved exclusively through the
        // pinned address, while the 302 response proves redirects stay disabled.
        let target = ResolvedTarget {
            domain: Some("rebind.invalid".to_string()),
            addresses: vec![address],
        };
        let client = build_pinned_client(&target).unwrap();
        let response = client
            .get(format!("http://rebind.invalid:{}/", address.port()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn redirect_targets_are_reparsed_and_private_hops_are_rejected() {
        let current = Url::parse("https://example.com/a/index.html").unwrap();
        let relative = HeaderValue::from_static("../next");
        let next = redirect_target(&current, StatusCode::FOUND, Some(&relative))
            .unwrap()
            .unwrap();
        assert_eq!(next.as_str(), "https://example.com/next");

        let private = HeaderValue::from_static("http://169.254.169.254/latest/meta-data");
        let next = redirect_target(&current, StatusCode::TEMPORARY_REDIRECT, Some(&private))
            .unwrap()
            .unwrap();
        assert!(resolve_target(&next).await.is_err());
    }

    #[tokio::test]
    async fn redirect_loop_rejects_private_second_hop_before_connecting() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).unwrap();
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                address.port()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let local_first_hop = format!("http://127.0.0.1:{}/public", address.port());
        let public_url = format!("http://public-hop.invalid:{}/start", address.port());
        let attempts = Arc::new(AtomicUsize::new(0));
        let observed_attempts = Arc::clone(&attempts);
        let result = get_public_with_sender(&public_url, move |requested| {
            let local_first_hop = local_first_hop.clone();
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                if requested.domain() == Some("public-hop.invalid") {
                    // The local transport stands in for an already validated
                    // public first hop. Automatic redirects remain disabled.
                    Client::builder()
                        .redirect(Policy::none())
                        .no_proxy()
                        .build()
                        .unwrap()
                        .get(local_first_hop)
                        .send()
                        .await
                        .map_err(|error| error.to_string())
                } else {
                    // The actual production hop sender must reject the private
                    // Location during resolution, before opening a connection.
                    send_public_hop(requested, "Demiurge SafeHttp Test", "*/*").await
                }
            }
        })
        .await;

        assert!(
            result.unwrap_err().contains("拒绝访问非公网地址"),
            "the private redirect target must fail address validation"
        );
        assert_eq!(observed_attempts.load(Ordering::SeqCst), 2);
        server.join().unwrap();
    }
}
