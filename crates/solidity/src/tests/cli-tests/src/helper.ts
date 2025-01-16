import * as shell from 'shelljs';
import * as fs from 'fs';
import { spawn } from 'child_process';

interface CommandResult {
    output: string;
    exitCode: number;
}

export const executeCommandWithStdin = (command: string, stdin: string): Promise<CommandResult> => {
    return new Promise((resolve, reject) => {
        const process = spawn(command, [], { shell: true });

        let stdout = '';
        let stderr = '';

        process.stdout.on('data', (chunk) => {
            stdout += chunk;
        });

        process.stderr.on('data', (chunk) => {
            stderr += chunk;
        });

        process.on('close', (exitCode) => {
            resolve({
                exitCode: exitCode || 0,
                output: (stdout || stderr).toString()
            });
        });

        process.on('error', (error) => {
            reject(new Error(`Failed to execute command: ${error.message}`));
        });

        process.stdin.write(stdin);
        process.stdin.end();
    });
};


export const executeCommand = (command: string): CommandResult => {
    const result = shell.exec(command, { silent: true, async: false });
    return {
        exitCode: result.code,
        output: result.stdout || result.stderr || ''
    };
};

export const isFolderExist = (folder: string): boolean => {
    return shell.test('-d', folder);
};

export const isFileExist = (pathToFileDir: string, fileName: string, fileExtension: string): boolean => {
    return shell.ls(pathToFileDir).stdout.includes(fileName + fileExtension);
};

export const isFileEmpty = (file: string): boolean => {
    if (fs.existsSync(file)) {
        return (fs.readFileSync(file).length === 0);
    }
};
