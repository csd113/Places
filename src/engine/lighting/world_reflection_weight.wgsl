    var weight = water_reflectance;
    if ((!water_blend_pass || in.water_surface <= 0.5)) {
        let facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
        let fresnel = mix(0.08, 1.0, pow(1.0 - facing, 5.0));
        let gloss = clamp(1.0 - material.roughness, 0.0, 1.0);
        let polish = gloss * gloss;
        weight = mix(fresnel * 0.35, 1.0, polish);
    }
