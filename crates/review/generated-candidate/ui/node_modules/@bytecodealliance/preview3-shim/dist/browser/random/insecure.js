import { insecure } from "@bytecodealliance/preview2-shim/random";
function getInsecureRandomBytes(maxLen) {
    return insecure.getInsecureRandomBytes(maxLen);
}
function getInsecureRandomU64() {
    return insecure.getInsecureRandomU64();
}
export default {
    getInsecureRandomBytes,
    getInsecureRandomU64,
};
