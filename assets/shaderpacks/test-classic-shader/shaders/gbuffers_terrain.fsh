#version 120

uniform sampler2D texture;
uniform sampler2D lightmap;

varying vec2 texcoord;
varying vec2 lmcoord;
varying vec4 glcolor;

void main() {
    vec4 color = texture2D(texture, texcoord) * glcolor;
    if (color.a < 0.1) discard;
    color.rgb *= texture2D(lightmap, lmcoord).rgb;
    /* DRAWBUFFERS:0 */
    gl_FragData[0] = color;
}
