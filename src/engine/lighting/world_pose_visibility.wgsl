
// The same source-wide authored distance calibration and finite tap pattern as
// TransportEmitter. Visibility is spatially interpolated at bounds anchors;
// receiver cosines and falloff use the actual transformed fragment position.
var<private> posed_fragment_normal: vec3<f32>;

fn self_shadow_bias_normal(in: VsOut, normal: vec3<f32>) -> vec3<f32> {
    let face = cross(dpdx(in.world_position), dpdy(in.world_position));
    let size = length(face);
    if (size <= 1.0e-12) { return normal; }
    let unit = face / size;
    return select(unit, -unit, dot(unit, normal) < 0.0);
}

// A fitted depth layer stores the actual current opaque pose for this exact
// finite-emitter tap. Compare against the receiving triangle's plane at each
// texel centre, avoiding a large slope bias through thin folds. Disagreement
// at a raster edge, unavailable projection, or grazing plane uses the exact ray.
fn posed_direct_visible(point:vec3<f32>, normal:vec3<f32>, source:vec3<f32>, root:f32, scale:f32, tap:u32)->bool {
    let base=environment._tail1;
    if(base>0.0){
        let index=u32(base-1.0)+tap;
        if(index<arrayLength(&posed_depth_samples)){
            let sample=posed_depth_samples[index];
            if(sample.parameters.y!=0u){
                let origin=point+normal*(0.0002*scale)-source;
                let clip=sample.projection*vec4<f32>(origin,1.0);
                let plane=transpose(sample.inverse)*vec4<f32>(normal,-dot(normal,origin));
                if(clip.w>0.0 && abs(plane.z)>1.0e-6*length(plane.xyz)){
                    let ndc=clip.xyz/clip.w;
                    let size=vec2<f32>(f32(sample.parameters.z));
                    let texel=(ndc.xy*vec2<f32>(0.5,-0.5)+vec2<f32>(0.5))*size-vec2<f32>(0.5);
                    let first=vec2<i32>(floor(texel));
                    if(ndc.z>=0.0 && ndc.z<=1.0 && all(first>=vec2<i32>(0)) && all(first+vec2<i32>(1)<vec2<i32>(i32(sample.parameters.z)))){
                        var visible=0u;
                        var valid=true;
                        for(var j=0u;j<4u;j+=1u){
                            let pixel=first+vec2<i32>(i32(j&1u),i32(j>>1u));
                            let xy=((vec2<f32>(pixel)+vec2<f32>(0.5))/size-vec2<f32>(0.5))*vec2<f32>(2.0,-2.0);
                            let receiver_depth=-(dot(plane.xy,xy)+plane.w)/plane.z;
                            valid=valid && receiver_depth>=0.0 && receiver_depth<=1.0;
                            let blocker_depth=textureLoad(posed_depth,pixel,i32(sample.parameters.x),0);
                            if(receiver_depth<=blocker_depth+0.0000005){visible|=1u<<j;}
                        }
                        if(valid && visible==15u){return true;}
                        if(valid && visible==0u){return false;}
                    }
                }
            }
        }
    }
    return posed_character_visible(point,normal,source,root,scale);
}

// Keep long visibility traversals out of the irradiance accumulator's live
// values. The same 32 source/tap answers are consumed by the unchanged lighting
// arithmetic below; zero-root receivers bypass the entire query phase.
fn entity_posed_visibility_mask(in: VsOut, normal: vec3<f32>, weights: EntityAnchorWeights) -> u32 {
    let root = environment.entity_bounds_extent.w;
    if (!POSED_DIRECT_ENABLED || root == 0.0) { return 0xffffffffu; }
    var mask = 0u;
    for (var index=0u; index<8u; index+=1u) {
        if (environment.entity_direct[index].color_strength.w <= 0.0) { continue; }
        let position = environment.entity_direct[index].position_range;
        let extent = environment.entity_direct[index].extent_falloff;
        let delta = position.xyz - in.world_position;
        let horizontal = max(abs(delta.xz)-extent.xy, vec2<f32>(0.0));
        let distance = select(length(delta), length(horizontal), extent.w>0.5);
        if (distance/max(position.w,1.0e-6)>=1.0) { continue; }
        let world_visibility = source_visibility_with_weights(weights,index);
        for (var tap=0u;tap<4u;tap+=1u) {
            if (world_visibility[tap] == 0.0) { continue; }
            let sample=environment.entity_direct[index].taps[tap];
            if (sample.w<=0.0) {continue;}
            let incoming=sample.xyz-in.world_position;
            let distance_to_tap=length(incoming);
            if (distance_to_tap>1.0e-6 && dot(normal,incoming/distance_to_tap)>0.0) {
                if (posed_direct_visible(in.world_position,posed_fragment_normal,sample.xyz,root,max(length(environment.model[0u].xyz),1.0e-6),index*4u+tap)) { mask |= 1u << (index*4u+tap); }
            }
        }
    }
    return mask;
}
