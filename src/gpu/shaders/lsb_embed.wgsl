// WGSL compute shader for parallel LSB embedding
// This shader embeds payload bits into the least significant bits of carrier data

@group(0) @binding(0)
var<storage, read_write> carrier_data: array<u32>;

@group(0) @binding(1)
var<storage, read> payload_bits: array<u32>;

// Workgroup size optimized for GPU performance
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    
    // Bounds check
    if (index >= arrayLength(&carrier_data)) {
        return;
    }
    
    // Calculate which payload bit to use
    let payload_index = index % arrayLength(&payload_bits);
    let bit_offset = index % 32u;
    
    // Extract the bit from payload
    let payload_word = payload_bits[payload_index / 32u];
    let payload_bit = (payload_word >> (payload_index % 32u)) & 1u;
    
    // Modify LSB of carrier data
    let carrier_word = carrier_data[index];
    let modified_word = (carrier_word & 0xFFFFFFFEu) | payload_bit;
    
    carrier_data[index] = modified_word;
}