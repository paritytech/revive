object "Test" {                                                                                                                                            
        code {                                                                                                                                                 
        function allocate(size) -> ptr {                                                                                                                       
            ptr := mload(0x40)                                                                                                                                 
            if iszero(ptr) { ptr := 0x60 }                                                                                                                     
            mstore(0x40, add(ptr, size))                                                                                                                       
        }                                                                                                                                                      
        let size := datasize("Test_deployed")          
        let offset := allocate(size)                                                                                                                           
        datacopy(offset, dataoffset("Test_deployed"), size)
        return(offset, size)                                                                                                                                   
    }                                                                          
    object "Test_deployed" {                                                                                                                                   
        code {                                                                                                                                                 
{                                                                                                                                                              
        let test:=0x5                                                                                                                                          
        mstore(2,signextend(0x8,0x0))                                                                                                                          
        mstore(8,lt(0xc,test))                                                 
}                                                                                                                                                              
                                                                                                                                                               
    return(0, 65536)                                                                                                                                           
}}}
