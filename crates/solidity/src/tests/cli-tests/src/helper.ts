import * as shell from 'shelljs';
import * as fs from 'fs';

interface CommandResult {
    output: string;
    exitCode: number;
}

export const executeCommand = (command: string): CommandResult  => {
    const result  = shell.exec(command, {async: false});
    return {
        exitCode: result.code,
        output: result.stdout.trim() || result.stderr.trim(),
    };
};

export const isFolderExist = (folder: string): boolean  => {
    return shell.test('-d', folder);
};

export const isFileExist = (pathToFileDir: string, fileName: string, fileExtension:string): boolean  => {     
    return shell.ls(pathToFileDir).stdout.includes(fileName + fileExtension); 
};

export const isFileEmpty = (file: string): boolean  => {
    if (fs.existsSync(file)) {
        return (fs.readFileSync(file).length === 0);
    } 
};
