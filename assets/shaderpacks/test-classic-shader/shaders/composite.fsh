#version 120

#define COLOR_BOOST 1.05 // [0.9 1.0 1.05 1.15 1.25]

uniform sampler2D colortex0;
varying vec2 texcoord;

void main() {
    vec3 color = texture2D(colortex0, texcoord).rgb;
    /* DRAWBUFFERS:0 */
    gl_FragData[0] = vec4(color * vec3(COLOR_BOOST, 1.0, 0.95), 1.0);
}

