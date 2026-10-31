import { insecureSeed } from "@bytecodealliance/preview2-shim/random";
function getInsecureSeed() {
    return insecureSeed.insecureSeed();
}
export default {
    getInsecureSeed,
};
