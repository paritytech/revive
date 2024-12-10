var Module = {
    stdinData: null,
    stdinDataPosition: 0,
    stdoutData: [],
    stderrData: [],

    // Function to read and return all collected stdout data as a string
    readFromStdout: function() {
        if (!this.stdoutData.length) return "";
        const decoder = new TextDecoder('utf-8');
        const data = decoder.decode(new Uint8Array(this.stdoutData));
        this.stdoutData = [];
        return data;
    },

    // Function to read and return all collected stderr data as a string
    readFromStderr: function() {
        if (!this.stderrData.length) return "";
        const decoder = new TextDecoder('utf-8');
        const data = decoder.decode(new Uint8Array(this.stderrData));
        this.stderrData = [];
        return data;
    },

    // Function to set input data for stdin
    writeToStdin: function(data) {
        const encoder = new TextEncoder();
        this.stdinData = encoder.encode(data);
        this.stdinDataPosition = 0;
    },

    // `preRun` is called before the program starts running
    preRun: function() {
        // Define a custom stdin function
        function customStdin() {
            if (!Module.stdinData || Module.stdinDataPosition >= Module.stdinData.length) {
                return null; // End of input (EOF)
            }
            return Module.stdinData[Module.stdinDataPosition++];
        }

        // Define a custom stdout function
        function customStdout(char) {
            Module.stdoutData.push(char);
        }

        // Define a custom stderr function
        function customStderr(char) {
            Module.stderrData.push(char);
        }

        FS.init(customStdin, customStdout, customStderr);
    },
};
