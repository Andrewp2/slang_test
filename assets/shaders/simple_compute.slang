module simple_compute;

RWStructuredBuffer<float> outputBuffer;

[shader("compute")]
[numthreads(1, 1, 1)]
void computeMain(uint3 dispatchThreadID: SV_DispatchThreadID)
{
    uint index = dispatchThreadID.x;
    // Write twice the index to the buffer
    outputBuffer[index] = float(index) * 2.0;
}
