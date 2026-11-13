import { random } from "@bytecodealliance/preview2-shim/random";
function getRandomBytes(maxLen) {
    return random.getRandomBytes(maxLen);
}
function getRandomU64() {
    return random.getRandomU64();
}
export default {
    getRandomBytes,
    getRandomU64,
};
