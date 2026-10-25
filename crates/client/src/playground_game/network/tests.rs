use super::*;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen(inline_js = "
export function stalled_probe(socket) {
  let sends = 0;
  Object.defineProperty(socket, 'bufferedAmount', { configurable: true, value: 1048576 });
  Object.defineProperty(socket, 'readyState', { configurable: true, value: 1 });
  socket.send = () => { sends++; };
  return () => { delete socket.readyState; return sends; };
}
")]
extern "C" {
    fn stalled_probe(socket: &WebSocket) -> js_sys::Function;
}

#[wasm_bindgen_test]
fn a_full_outgoing_socket_closes_without_sending_another_frame() {
    let socket = WebSocket::new("ws://127.0.0.1:18767").unwrap();
    let probe = stalled_probe(&socket);
    assert!(send_bytes(&socket, &[1]).is_err());
    assert_eq!(probe.call0(&JsValue::NULL).unwrap().as_f64(), Some(0.0));
    assert!(matches!(
        socket.ready_state(),
        WebSocket::CLOSING | WebSocket::CLOSED
    ));
}

#[wasm_bindgen_test]
fn incoming_frames_reject_text_and_oversize_before_copying_to_wasm() {
    // A numeric string must never become a typed-array allocation length.
    assert!(incoming_bytes(&JsValue::from_str("1073741824")).is_err());
    let oversized = js_sys::ArrayBuffer::new(aoe_protocol::GAMEPLAY_MAX_MESSAGE as u32 + 1);
    assert!(incoming_bytes(oversized.as_ref()).is_err());
    let buffer = js_sys::ArrayBuffer::new(3);
    Uint8Array::new(&buffer).copy_from(&[1, 2, 3]);
    assert_eq!(incoming_bytes(buffer.as_ref()).unwrap(), [1, 2, 3]);
}

#[wasm_bindgen_test]
fn retiring_a_connection_detaches_and_drops_all_three_callbacks() {
    let socket = WebSocket::new("ws://127.0.0.1:18767").unwrap();
    let owner = Rc::new(());
    let opened = owner.clone();
    let onopen = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = &opened;
    });
    let received = owner.clone();
    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |_| {
        let _ = &received;
    });
    let closed = owner.clone();
    let onclose = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = &closed;
    });
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    let callbacks = ConnectionCallbacks {
        socket: socket.clone(),
        _onopen: onopen,
        _onmessage: onmessage,
        _onclose: onclose,
    };
    assert_eq!(Rc::strong_count(&owner), 4);
    drop(callbacks);
    assert_eq!(Rc::strong_count(&owner), 1);
    assert!(socket.onopen().is_none());
    assert!(socket.onmessage().is_none());
    assert!(socket.onclose().is_none());
    assert!(matches!(
        socket.ready_state(),
        WebSocket::CLOSING | WebSocket::CLOSED
    ));
}
