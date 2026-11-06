#include <libusb.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static libusb_device_handle *h;
static void drain(void){unsigned char t[64];int n;while(!libusb_bulk_transfer(h,0x82,t,64,&n,50)&&n>0);}
static int out(const char*s){int n;return libusb_bulk_transfer(h,0x01,(unsigned char*)s,strlen(s),&n,1000);}
int main(int c,char**v){ /* load <bin> <addr>; then go */
 libusb_init(0); h=libusb_open_device_with_vid_pid(0,0x03eb,0x6124);
 if(!h){puts("no device");return 1;}
 libusb_detach_kernel_driver(h,1); if(libusb_claim_interface(h,1)){puts("claim failed");return 1;}
 out("N#"); drain();
 FILE*f=fopen(v[1],"rb"); unsigned a=strtoul(v[2],0,16); unsigned w; int n=0;
 while(1){w=0; size_t r=fread(&w,1,4,f); if(!r)break; char s[40]; snprintf(s,sizeof s,"W%08X,%08X#",a+4*n,w); if(out(s)){printf("write fail at %d\n",n);return 1;} n++; usleep(2000);}
 printf("loaded %d words\n",n); drain();
 char g[24]; snprintf(g,sizeof g,"G%08X#",a); out(g); puts("go sent"); return 0;}
