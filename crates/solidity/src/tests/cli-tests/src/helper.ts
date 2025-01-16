import * as shell from 'shelljs';
import * as fs from 'fs';
import { spawnSync } from 'child_process';

interface CommandResult {
    output: string;
    exitCode: number;
}

export const executeCommand = (command: string, stdin?: string): CommandResult => {
    if (stdin) {
        const process = spawnSync(command, [], {
            input: stdin,
            shell: true,
            encoding: 'utf-8'
        });

        return {
            exitCode: process.status || 0,
            output: (process.stdout || process.stderr || '').toString()
        };
    }

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
