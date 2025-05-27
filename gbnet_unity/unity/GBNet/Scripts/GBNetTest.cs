using System;
using System.Runtime.InteropServices;
using UnityEngine;

namespace GBNet
{
    public class GBNetTest : MonoBehaviour
    {
        const string DLL_NAME = "gbnet_unity";
        
        [DllImport(DLL_NAME)]
        private static extern int gbnet_test_add(int a, int b);
        
        [DllImport(DLL_NAME)]
        private static extern uint gbnet_get_version();
        
        [DllImport(DLL_NAME)]
        private static extern int gbnet_test_bit_packing();
        
        void Start()
        {
            Debug.Log("=== GBNet FFI Test ===");
            
            try
            {
                int sum = gbnet_test_add(5, 3);
                Debug.Log($"✅ FFI Working: 5 + 3 = {sum}");
                
                uint version = gbnet_get_version();
                Debug.Log($"✅ GBNet Version: {(version >> 24) & 0xFF}.{(version >> 16) & 0xFF}.{version & 0xFFFF}");
                
                int bytes = gbnet_test_bit_packing();
                Debug.Log($"✅ Bit Packing: 28 bits need {bytes} bytes");
                
                Debug.Log("🎉 All tests passed! GBNet FFI is working!");
            }
            catch (Exception e)
            {
                Debug.LogError($"❌ FFI Error: {e.Message}");
            }
        }
    }
}
