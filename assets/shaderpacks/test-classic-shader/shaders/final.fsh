#version 120

#define VignetteStrength 0.7 // [0.0 0.2 0.4 0.6 0.8 1.0]
#define VIGNETTE

uniform sampler2D colortex0;
varying vec2 texcoord;

void main() {
    vec3 color = texture2D(colortex0, texcoord).rgb;
    vec2 centred = texcoord - 0.5;
#ifdef VIGNETTE
    float vignette = 1.0 - dot(centred, centred) * VignetteStrength;
    color *= vignette;
#endif
    gl_FragColor = vec4(color, 1.0);
}

