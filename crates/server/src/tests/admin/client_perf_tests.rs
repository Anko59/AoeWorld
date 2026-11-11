use super::*;

async fn post(server: &Server, token: &str, body: Value) -> u16 {
    let mut stream = TcpStream::connect(server.address).await.unwrap();
    let body = body.to_string();
    let request = format!(
        "POST /client/perf HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nx-aoeworld-controller-token: {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response.split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn report() -> Value {
    json!({"frames":120, "frame_ms":[16.0,32.0,80.0],
        "stages_ms": {"update":[1.0,2.0,3.0], "interpolation":[0.0,1.0,2.0],
        "culling":[0.0,1.0,2.0], "terrain":[0.0,1.0,2.0],
        "sprites":[1.0,2.0,3.0], "ui":[0.0,1.0,2.0], "fog":[0.0,1.0,2.0]},
        "draw_calls":[2.0,4.0,8.0], "renderer":"webgl2",
        "contour": {"effect_frames":30, "plain_frames":90, "eligible_instances":12,
        "occluder_instances":40, "depth_span":180.5, "context_lost":1, "context_restored":1}})
}

#[tokio::test]
async fn authenticated_client_timings_are_bounded_rate_limited_and_visible_with_ticks() {
    let gameplay = villages();
    let token = gameplay
        .slot_infos()
        .await
        .into_iter()
        .find(|s| s.human)
        .unwrap()
        .token;
    let token: String = token.0.iter().map(|b| format!("{b:02x}")).collect();
    let server = server(gameplay, true).await;
    assert_eq!(post(&server, "wrong", report()).await, 403);
    let mut invalid = report();
    invalid["frame_ms"] = json!([32, 16, 80]);
    assert_eq!(post(&server, &token, invalid).await, 400);
    let mut invalid = report();
    invalid["frames"] = json!(100_000);
    assert_eq!(post(&server, &token, invalid).await, 400);
    let mut invalid = report();
    invalid["renderer"] = json!("x".repeat(5000));
    assert_eq!(post(&server, &token, invalid).await, 413);
    assert_eq!(post(&server, &token, report()).await, 204);
    assert_eq!(post(&server, &token, report()).await, 429);
    let (status, body) = request(&server, "GET", "/admin/perf", Some(SECRET), "").await;
    assert_eq!(status, 200);
    assert_eq!(body["clients"].as_array().unwrap().len(), 1);
    assert_eq!(body["clients"][0]["frame_ms"], json!([16.0, 32.0, 80.0]));
    assert_eq!(body["clients"][0]["contour"]["effect_frames"], 30);
    assert_eq!(body["clients"][0]["contour"]["context_lost"], 1);
    assert!(body.get("samples").is_some(), "tick summary retained");
    assert!(
        !body.to_string().contains(&token),
        "never expose resume tokens"
    );
}
