return Module;
}
if (typeof module === "object" && typeof module.exports === "object") {
  module.exports = createRevive;
} else if (typeof define === "function" && define["amd"])
  define([], () => createRevive);
