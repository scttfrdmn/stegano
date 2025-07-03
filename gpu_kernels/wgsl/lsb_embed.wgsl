// GPU compute shader for parallel LSB embedding
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@group(0) @binding(1) var<storage, read> payload_bits: array<u32>;
@group(0) @binding(2) var<uniform> params: EmbedParams;

struct EmbedParams {
    data_length: u32,
    payload_length: u32,
    bits_per_element: u32,
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    
    if (index >= params.data_length) {
        return;
    }
    
    let bit_index = index % params.payload_length;
    let payload_bit = (payload_bits[bit_index / 32u] >> (bit_index % 32u)) & 1u;
    
    // Clear LSB and set new bit
    data[index] = (data[index] & 0xFFFFFFFEu) | payload_bit;
}
