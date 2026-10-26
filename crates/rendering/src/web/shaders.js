export async function fetch_shaders() {
  return Promise.all(['ordinary', 'mixing'].map(async name => {
    const response = await fetch(`/pkg/${name}.wgsl`);
    if (!response.ok) throw Error('WebGPU shader download failed');
    return response.text();
  }));
}
