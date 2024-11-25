var Module = {
    stdinData: "",
    stdoutCallback: null,
    stderrCallback: null,

    // Function to set a callback for stdout
    setStdoutCallback: function(callback) {
        this.stdoutCallback = callback;
    },

    // Function to set a callback for stderr
    setStderrCallback: function(callback) {
        this.stderrCallback = callback;
    },

    // Function to set input data for stdin
    setStdinData: function(data) {
        this.stdinData = data;
    },

    // `preRun` is called before the program starts running
    preRun: function() {
        // Define a custom stdin function
        function customStdin() {
            if (Module.stdinData.length === 0) {
                return null; // End of input (EOF)
            }
            const char = Module.stdinData.charCodeAt(0);
            Module.stdinData = Module.stdinData.slice(1); // Remove the character from input
            return char;
        }

        // Define a custom stdout function
        function customStdout(char) {
            if (Module.stdoutCallback) {
                Module.stdoutCallback(String.fromCharCode(char));
            }
        }

        // Define a custom stderr function
        function customStderr(char) {
            if (Module.stderrCallback) {
                Module.stderrCallback(String.fromCharCode(char));
            }
        }

        FS.init(customStdin, customStdout, customStderr);
    },
};
