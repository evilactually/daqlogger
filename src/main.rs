use core::ffi::c_char;
use std::ffi::CString;
use ni_daqmx_sys;

static SAMPLES_PER_SECOND : ni_daqmx_sys::float64 = 1000.0;
static SAMPLES: i32 = 1000;
static CHANNELS: i32 = 2;

macro_rules! check_err {
    ($prefix:expr,$err:expr) => {
        if $err != 0 {
            eprintln!("{} error: {:?}", $prefix, $err);
        }
    };
}


fn main() {
    unsafe {
        let mut task_handle : ni_daqmx_sys::TaskHandle = std::ptr::null_mut();
        //let ch : c_str
        //let task_name: *const c_char = CString::new("daq01").expect("CString::new failed").as_ptr();

        let err = ni_daqmx_sys::DAQmxCreateTask(std::ptr::null(), &mut task_handle);
        check_err!("DAQmxCreateTask", err);
        let ch_name = CString::new("cDAQ9181-1FE3677Mod1/ai0, cDAQ9181-1FE3677Mod1/ai8").expect("CString::new failed");
        let ch_name_ptr: *const c_char = ch_name.as_ptr();
        let err = ni_daqmx_sys::DAQmxCreateAIVoltageChan(task_handle, ch_name_ptr, std::ptr::null(), ni_daqmx_sys::DAQmx_Val_RSE, -10.0, 10.0, ni_daqmx_sys::DAQmx_Val_Volts, std::ptr::null());
        check_err!("DAQmxCreateAIVoltageChan", err);
        let err = ni_daqmx_sys::DAQmxCfgSampClkTiming(task_handle, std::ptr::null(), SAMPLES_PER_SECOND, ni_daqmx_sys::DAQmx_Val_Rising, ni_daqmx_sys::DAQmx_Val_FiniteSamps, 1000);
        check_err!("DAQmxCfgSampClkTiming", err);
        let err = ni_daqmx_sys::DAQmxStartTask(task_handle);
        check_err!("DAQmxStartTask", err);
        let mut data : [ni_daqmx_sys::float64; (CHANNELS*SAMPLES) as usize] = [0.0; (CHANNELS*SAMPLES) as usize];
        let data_ptr: *mut f64 = data.as_mut_ptr();
        let mut read : i32 = -1;
        let err = ni_daqmx_sys::DAQmxReadAnalogF64(task_handle, SAMPLES, 10.0, ni_daqmx_sys::DAQmx_Val_GroupByScanNumber as u32, data_ptr, (CHANNELS*SAMPLES) as u32, &mut read, std::ptr::null_mut());
        check_err!("DAQmxReadAnalogF64", err);
        //println!("DAQmxReadAnalogF64 {:?}", data);
        let err = ni_daqmx_sys::DAQmxStopTask(task_handle);
        check_err!("DAQmxStopTask", err);
        println!("{}", read);

        // for i in 0..data.len() {
        //     println!("{}", data[i]);
        // }

        for i in 0..data.len()/2 {
            let j = i*2;
            println!("{} {}", data[j], data[j+1]);
        }

    }
    
}
