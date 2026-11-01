// Reserve one exact bounded chunk, then cancel incomplete or excessive streams.
export async function fetch_chunk(name, expected) {
    const file = name.startsWith('buildings/') ? name.slice(10) : name;
    if (!/^[a-zA-Z0-9_.-]+\.bin$/.test(file) ||
        !Number.isInteger(expected) || expected <= 0 || expected > 67108864)
        throw Error('Invalid native chunk request');
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 10000);
    let reader;
    try {
        const response = await fetch('/asset-pack/' + name, {signal:controller.signal});
        if (!response.ok || !response.body) throw Error('Native chunk unavailable');
        const length = response.headers.get('Content-Length');
        if (length !== null && Number(length) !== expected) throw Error('Native chunk length mismatch');
        reader = response.body.getReader();
        const output = new Uint8Array(expected);
        let at = 0;
        for (;;) {
            const {done,value} = await reader.read();
            if (done) break;
            if (!(value instanceof Uint8Array) || value.length > expected - at)
                throw Error('Native chunk exceeds byte budget');
            output.set(value,at); at += value.length;
        }
        if (at !== expected) throw Error('Truncated native chunk');
        return output;
    } finally {
        clearTimeout(timer);
        controller.abort();
        if (reader) await reader.cancel().catch(() => {});
    }
}
