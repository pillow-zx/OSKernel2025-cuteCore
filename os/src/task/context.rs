use crate::hal::trap_return;

#[repr(C)]
pub struct TaskContext {
    // 返回地址，在la中应该为$ra
    ra: usize,
    // 栈指针，在la中应该为$sp
    sp: usize,
    // 通用寄存器，在la中应该为$s0~$s8
    s: [usize; 12],
}

impl TaskContext {
    // 空初始化
    pub fn zero_init() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s: [0; 12],
        }
    }
    // 从指定栈指针和返回地址初始化
    pub fn goto_trap_return(kstack_ptr: usize) -> Self {
        Self {
            ra: trap_return as usize,
            sp: kstack_ptr,
            s: [0; 12],
        }
    }
}
